use std::{collections::HashMap, sync::Arc, time::Instant};

use closest::{GetOrHelp, ResourceKind};
use config_types::{TenantInfo, WebConfig};
use conflux::{
    ACodec, Asset, BitmapVariant, Derivation, DerivationBitmap, DerivationDrawioRender,
    DerivationIdentity, DerivationKind, DerivationPassthrough, DerivationSvgCleanup,
    DerivationVideo, DerivationVideoThumbnail, InputPathRef, LoadedPage, MarkdownRef, Media,
    MediaKind, Page, PageKind, Pak, Part, PartNumber, PathMappings, Revision, Route, SeriesLink,
    VCodec, VContainer, VideoInfo, VideoVariant,
};
use content_type::ContentType;
use cub_types::IndexedRevision;
use derivations::DerivationInfo;
use eyre::{Context, eyre};
use image_types::{ICodec, LogicalPixels, PixelDensity};
use itertools::Itertools;
use libsearch::Index;
use markdown_types::ProcessMarkdownArgs;
use merde::{DynDeserializerExt, yaml::YamlDeserializer};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use template_types::{CompileArgs, TemplateCollection};
use tracing::{self, debug, warn};

use crate::impls::frontmatter::{Frontmatter, FrontmatterIn};

pub async fn load_pak(
    pak: Pak,
    ti: Arc<TenantInfo>,
    prev_rev: Option<&Revision>,
    mappings: PathMappings,
    web: WebConfig,
) -> eyre::Result<IndexedRevision> {
    let mut rev = Revision {
        pak,
        ti: ti.clone(),
        pages: Default::default(),
        page_routes: Default::default(),
        assets: Default::default(),
        asset_routes: Default::default(),
        tags: Default::default(),
        media: Default::default(),
        mappings,
    };

    let before_input = Instant::now();
    for (input_path, input) in &rev.pak.inputs {
        let (base, _ext) = input_path.explode();

        if input_path.as_str().starts_with("/dist/") {
            // insert as is, derivation hash is the same as the input hash
            let d = Derivation {
                input: input.path.clone(),
                kind: DerivationKind::Passthrough(DerivationPassthrough {}),
            };
            let dinfo = DerivationInfo::new(input, &d);

            tracing::debug!(
                "For dist asset, inserting passthrough derivation: {input_path} => {dinfo:#?}"
            );
            rev.assets
                .insert(dinfo.route(), Asset::Derivation(d.clone()));

            // additionally, for some entry points, we add a hashless route
            struct Exception {
                pattern: &'static str,
                input_path_key: &'static InputPathRef,
            }
            let exceptions = [
                Exception {
                    pattern: "/dist/assets/bundle-*.js",
                    input_path_key: InputPathRef::from_str("/dist/assets/bundle.js"),
                },
                Exception {
                    pattern: "/dist/assets/bundle-*.css",
                    input_path_key: InputPathRef::from_str("/dist/assets/bundle.css"),
                },
                Exception {
                    pattern: "/dist/assets/index-*.js",
                    input_path_key: InputPathRef::from_str("/dist/assets/index.js"),
                },
                Exception {
                    pattern: "/dist/assets/index-*.css",
                    input_path_key: InputPathRef::from_str("/dist/assets/index.css"),
                },
            ];
            for ex in exceptions.iter() {
                if wildmatch::WildMatch::new(ex.pattern).matches(input_path.as_str()) {
                    rev.asset_routes
                        .insert(ex.input_path_key.to_owned(), dinfo.route());
                }
            }

            continue;
        }

        if input.path.as_str().ends_with("/index.md") {
            return Err(eyre!(
                "As of `home` v33.0.0, the special name for index pages is '_index.md' for consistency with Zola. Please rename '{}'.",
                input.path
            ));
        }

        match input.content_type {
            ContentType::WOFF2 => {
                // insert as is, derivation hash is the same as the input hash
                let d = Derivation {
                    input: input.path.clone(),
                    kind: DerivationKind::Identity(DerivationIdentity {}),
                };
                let dinfo = DerivationInfo::new(input, &d);
                rev.assets.insert(dinfo.route(), Asset::Derivation(d));
            }
            ContentType::WASM | ContentType::Js | ContentType::CSS => {
                // we serve everything pass-through
                let dkind = DerivationKind::Passthrough(DerivationPassthrough {});
                let d = Derivation {
                    input: input.path.clone(),
                    kind: dkind,
                };
                let dinfo = DerivationInfo::new(input, &d);
                rev.assets
                    .insert(dinfo.route(), Asset::Derivation(d.clone()));
            }
            ContentType::JsSourcemap => {
                // sourcemaps are a special case, we don't rewrite .js files to rewrite the
                // source map URL, so we serve source maps without cache-busting
                let route_path = Route::new(format!("{base}.map"));
                rev.assets.insert(
                    route_path,
                    Asset::Derivation(Derivation {
                        input: input.path.clone(),
                        kind: DerivationKind::Identity(DerivationIdentity {}),
                    }),
                );
            }
            _other => {
                continue;
            }
        }
    }

    recompute_asset_routes(&mut rev)?;

    // This is where we decide which variants we'll build of various media
    for (path, props) in &rev.pak.media_props {
        let mut media = Media::new(props.clone());
        let input = rev
            .pak
            .inputs
            .get(path)
            .ok_or_else(|| eyre!("media without input: {path}, props = {props:#?}"))?;

        match props.kind {
            MediaKind::Bitmap => {
                let src_codec = media.props.ic.ok_or_else(|| {
                    eyre!("bitmap media without codec: {path}, props = {props:#?}")
                })?;

                // in CSS 'w' units
                let intrinsic_source_width = media.props.dims.w;

                let target_widths = [
                    Some(LogicalPixels::from(400.0)),
                    Some(LogicalPixels::from(900.0)),
                    None,
                ];
                let target_densities = [PixelDensity::ONE, PixelDensity::TWO]; // sorry higher pixel densities :(

                for dst_codec in [ICodec::JXL, ICodec::AVIF, ICodec::WEBP, ICodec::PNG] {
                    for target_width in target_widths.iter().copied() {
                        let mut bitmap_variant = BitmapVariant {
                            ic: dst_codec,
                            max_width: target_width,
                            srcset: Default::default(),
                        };

                        for target_density in target_densities.iter().copied() {
                            // we might skip on this one if the source image is too small!
                            if let Some(target_width) = target_width {
                                let intrinsic_target_width =
                                    target_width.to_intrinsic(target_density);
                                if intrinsic_source_width < intrinsic_target_width {
                                    // let's not upscale the image — it's pointless.
                                    continue;
                                }
                            }

                            let derivation = Derivation {
                                input: path.clone(),
                                kind: if src_codec == dst_codec && target_width.is_none() {
                                    DerivationKind::Identity(DerivationIdentity {})
                                } else {
                                    DerivationKind::Bitmap(DerivationBitmap {
                                        ic: dst_codec,
                                        width: target_width
                                            .as_ref()
                                            .map(|w| w.to_intrinsic(target_density)),
                                    })
                                },
                            };
                            let dinfo = DerivationInfo::new(input, &derivation);
                            let route = dinfo.route();
                            rev.assets
                                .insert(route.clone(), Asset::Derivation(derivation));
                            bitmap_variant.srcset.push((target_density, route));
                        }

                        if !bitmap_variant.srcset.is_empty() {
                            media = media.with_bitmap_variant(bitmap_variant);
                        }
                    }
                }
            }
            MediaKind::Video => {
                for (dst_container, dst_vc, dst_ac) in [
                    // AV1 needs to be first to be the default
                    (VContainer::MP4, VCodec::AV1, ACodec::Opus),
                    (VContainer::WebM, VCodec::VP9, ACodec::Opus),
                ] {
                    let is_identity = {
                        if let (Some(src_vc), Some(src_ac)) = (props.vc(), props.ac()) {
                            src_vc == dst_vc && src_ac == dst_ac
                        } else {
                            false
                        }
                    };

                    let derivation = Derivation {
                        input: path.clone(),
                        kind: if is_identity {
                            DerivationKind::Identity(DerivationIdentity {})
                        } else {
                            DerivationKind::Video(DerivationVideo {
                                container: dst_container,
                                vc: dst_vc,
                                ac: dst_ac,
                            })
                        },
                    };
                    let dinfo = DerivationInfo::new(input, &derivation);
                    let route = dinfo.route();
                    rev.assets
                        .insert(route.clone(), Asset::Derivation(derivation));
                    media = media.with_video_variant(VideoVariant {
                        container: dst_container,
                        vc: dst_vc,
                        ac: dst_ac,
                        route,
                    });
                }

                // now take care of the thumbnail (in multiple variants)
                let mut thumb_options: Vec<(ContentType, Route)> = vec![];

                for ic in [ICodec::JXL, ICodec::WEBP, ICodec::AVIF] {
                    let derivation = Derivation {
                        input: path.clone(),
                        kind: DerivationKind::VideoThumbnail(DerivationVideoThumbnail { ic }),
                    };
                    let dinfo = DerivationInfo::new(input, &derivation);
                    let route = dinfo.route();
                    rev.assets
                        .insert(route.clone(), Asset::Derivation(derivation));
                    thumb_options.push((ic.content_type(), route));
                }

                let (base, _ext) = path.explode();
                let thumb_route = Route::new(format!("{base}.thumb"));
                rev.assets.insert(
                    thumb_route.clone(),
                    Asset::AcceptBasedRedirect {
                        options: thumb_options,
                    },
                );
                media.thumb = Some(thumb_route);
            }
            MediaKind::Audio => {
                // for now they're just m4a files served as-is, so.. just do an
                // identity derivation
                let derivation = Derivation {
                    input: path.clone(),
                    kind: DerivationKind::Identity(DerivationIdentity {}),
                };
                let dinfo = DerivationInfo::new(input, &derivation);
                let route = dinfo.route();
                rev.assets
                    .insert(route.clone(), Asset::Derivation(derivation));
            }
            MediaKind::Diagram => {
                let derivation = match input.content_type {
                    ContentType::DrawIO => Derivation {
                        input: path.clone(),
                        kind: DerivationKind::DrawioRender(DerivationDrawioRender {
                            svg_font_face_collection: rev.pak.svg_font_face_collection.clone(),
                        }),
                    },
                    ContentType::SVG => Derivation {
                        input: path.clone(),
                        kind: DerivationKind::SvgCleanup(DerivationSvgCleanup {}),
                    },
                    _ => {
                        return Err(eyre!(
                            "Unsupported content type for media diagram: {} (path: {})",
                            input.content_type,
                            path
                        ));
                    }
                };

                let dinfo = DerivationInfo::new(input, &derivation);
                let route = dinfo.route();
                rev.assets
                    .insert(route.clone(), Asset::Derivation(derivation));
            }
        }

        rev.media.insert(path.clone(), media);
    }
    recompute_asset_routes(&mut rev)?;

    tracing::debug!(
        "Loaded {} inputs in {:?}",
        rev.pak.inputs.len(),
        before_input.elapsed()
    );

    let nonbusted_routes = vec![
        InputPathRef::from_str("/content/img/logo-square.png"),
        InputPathRef::from_str("/content/img/logo-square-2.png"),
        InputPathRef::from_str("/content/img/logo-round.png"),
        InputPathRef::from_str("/content/img/logo-round-2.png"),
    ];

    for nonbusted_input_path in nonbusted_routes {
        if let Some(route_path) = rev.asset_routes.get(nonbusted_input_path) {
            // add a route that's not cache-busted for some specific assets (which
            // may be requested by RSS clients, etc.)
            if let Some(asset_route) = rev.assets.get(route_path) {
                if let Some(rest) = nonbusted_input_path.as_str().strip_prefix("/content") {
                    let nonbusted_route_path = Route::new(rest.to_string());
                    rev.assets.insert(nonbusted_route_path, asset_route.clone());
                }
            }
        }
    }

    let template_start = Instant::now();
    let templates = lrev_make_template_collection(&mut rev).wrap_err("compiling templates")?;
    tracing::debug!("Compiled templates in {:?}", template_start.elapsed());

    let markdown_load_start = Instant::now();
    let mod_markdown = libmarkdown::load();
    tracing::debug!(
        "Loaded markdown module in {:?}",
        markdown_load_start.elapsed()
    );

    let page_sort_start = Instant::now();
    let mut input_pages: Vec<&InputPathRef> = rev.pak.pages.keys().map(|p| p.as_ref()).collect();
    input_pages.sort_by_key(|p| p.as_str());
    tracing::debug!(
        "Sorted {} input pages in {:?}",
        input_pages.len(),
        page_sort_start.elapsed()
    );

    let mut reused: Vec<Arc<LoadedPage>> = Vec::new();
    let mut to_build: Vec<Page> = Vec::new();

    let before_calculate_deps = Instant::now();
    for path in input_pages {
        let page = rev.pak.pages.get(path).unwrap();
        let route_path = path.to_route_path();
        rev.page_routes
            .insert(route_path.to_owned(), page.path.clone());

        let prev_page = {
            if let Some(prev) = prev_rev {
                if let Some(prev_page) = prev.pak.pages.get(path) {
                    if prev_page.hash == page.hash {
                        // check if any of the dependencies have changed
                        let deps_changed = prev_page.deps.iter().any(|dep| {
                            let prev_input = prev.pak.inputs.get(dep);
                            let curr_input = rev.pak.inputs.get(dep);
                            let prev_input_hash = prev_input.map(|input| &input.hash);
                            let curr_input_hash = curr_input.map(|input| &input.hash);
                            if prev_input_hash.is_none() || curr_input_hash.is_none() {
                                tracing::warn!("hash is none for {dep}");
                            }
                            let different = prev_input_hash != curr_input_hash;
                            if different {
                                tracing::info!(
                                    "For \x1b[32m{path:?}\x1b[0m\n\
                                     dep \x1b[33m{dep}\x1b[0m hash went \x1b[31m{prev_input_hash:?}\x1b[0m => \x1b[32m{curr_input_hash:?}\x1b[0m"
                                );
                            } else {
                                tracing::trace!("Page \x1b[32m{path:?}\x1b[0m has the same hash for {dep}");
                            }
                            different
                        });

                        if deps_changed {
                            tracing::trace!(
                                "Page \x1b[32m{path:?}\x1b[0m has a changed dep, not re-using"
                            );
                            None
                        } else {
                            tracing::trace!(
                                "Page \x1b[32m{path:?}\x1b[0m has not changed and none of its {} deps have changed, re-using",
                                prev_page.deps.len()
                            );
                            prev.pages.get(&prev_page.path)
                        }
                    } else {
                        tracing::trace!("Page \x1b[32m{path:?}\x1b[0m has changed, not re-using");
                        None
                    }
                } else {
                    tracing::trace!(
                        "Page \x1b[32m{path:?}\x1b[0m not found in previous revision, not re-using"
                    );
                    None
                }
            } else {
                tracing::trace!("No previous revision available, not re-using any pages");
                None
            }
        };

        if let Some(prev_page) = prev_page {
            reused.push(prev_page.clone());
        } else {
            to_build.push(page.clone());
        }
    }
    tracing::debug!(
        "Calculated dependencies in {:?}",
        before_calculate_deps.elapsed()
    );

    let start = Instant::now();

    // well that's a hack, but... the templating stuff needs to have an `Arc<dyn RevisionView>` to gnaw on
    // for now, so until we disentangle the dependencies here, that'll be that.
    let rev = Arc::new(rev);

    let mut to_reinsert: Vec<Page> = Vec::new();

    // Collect dependencies for each page to be built
    for page in &mut to_build {
        let deps_result = mod_markdown.collect_dependencies(ProcessMarkdownArgs {
            path: &page.path,
            markdown: MarkdownRef::from_str(&page.markup),
            w: &mut Vec::new(),
            rv: rev.clone(),
            ti: rev.ti.clone(),
            templates: templates.as_ref(),
            web,
        })?;

        page.deps = deps_result.deps.into_iter().collect();
        page.deps.sort();

        to_reinsert.push(page.clone());
    }

    // whoop whoop, transition away from Arc for a bit because we need to track dependencies in pages...
    let mut rev = Arc::try_unwrap(rev).unwrap();
    for page in to_reinsert {
        rev.pak.pages.insert(page.path.clone(), page);
    }
    let rev = Arc::new(rev);

    let mut built: Vec<LoadedPage> = to_build
        .par_iter()
        .map(|page| load_single_page(rev.clone(), templates.as_ref(), page, mod_markdown, web))
        .collect::<Result<_, _>>()?;

    let mut rev =
        Arc::into_inner(rev).expect("no references to the revision must be kept after building");

    let built_pages = built.len();
    let total_pages = built.len() + reused.len();
    let elapsed = start.elapsed();
    tracing::info!(
        "Built \x1b[32m{built_pages}\x1b[0m/\x1b[33m{total_pages}\x1b[0m pages in \x1b[36m{elapsed:?}\x1b[0m",
    );

    let series_links_start = Instant::now();
    for lpage in &mut built {
        mark_series_links(lpage)?;
    }

    tracing::debug!(
        "Marked series links for {} pages in {:?}",
        built.len(),
        series_links_start.elapsed()
    );

    // Extend lrev.pages with newly built and reused pages
    let extend_start = Instant::now();
    rev.pages.extend(
        built
            .into_iter()
            .map(Arc::new)
            .chain(reused)
            .map(|p| (p.path.clone(), p)),
    );
    tracing::debug!("Extended pages in {:?}", extend_start.elapsed());

    let aliases_start = Instant::now();
    for page in rev.pages.values() {
        for alias in &page.aliases {
            rev.page_routes.insert(alias.clone(), page.path.clone());
        }
    }
    tracing::debug!(
        "Added page aliases to routes in {:?}",
        aliases_start.elapsed()
    );

    // Now collect series parts
    let series_parts_start = Instant::now();
    let mut series_parts: HashMap<Route, HashMap<PartNumber, Arc<LoadedPage>>> = Default::default();

    for lpage in rev.pages.values() {
        if let Some(link) = &lpage.series_link {
            series_parts
                .entry(link.index_route.clone())
                .or_default()
                .insert(link.part_number, lpage.clone());
        }
    }
    tracing::debug!(
        "Collected series parts in {:?}",
        series_parts_start.elapsed()
    );

    for (index_route, parts) in series_parts {
        let index_path = rev
            .page_routes
            .get_or_help(ResourceKind::Route, &index_route)
            .wrap_err("looking up series index route")?
            .clone();
        let index_page = rev
            .pages
            .get_or_help(ResourceKind::Page, &index_path)
            .wrap_err("looking up series index page")?;

        if index_page.kind != PageKind::SeriesIndex {
            eyre::bail!(
                "Expected page for series index path {index_route} to be a SeriesIndex, but it was a {index_page:?}"
            );
        };

        let num_parts = parts.len();
        let mut parts_vec: Vec<Part> = Vec::with_capacity(parts.len());

        let mut total_reading_time = 0;
        for i in 1..=num_parts {
            let part = parts.get(&PartNumber::new(i)).ok_or_else(|| {
                eyre!(
                    "Could not find part {i} of series {index_route}. All part numbers: {:?}",
                    parts.keys().join(", ")
                )
            })?;
            let part_path = part.path.clone();
            let part_route = part_path.to_route_path().to_owned();

            total_reading_time += part.reading_time;
            parts_vec.push(Part {
                title: part.title.clone(),
                path: part_path,
                route: part_route,
            });
        }

        let mut index_page_cloned = LoadedPage::clone(index_page.as_ref());
        index_page_cloned.reading_time = total_reading_time;
        index_page_cloned.parts = parts_vec;

        // replace the index page with the new one
        rev.pages
            .insert(index_path.clone(), Arc::new(index_page_cloned));
    }

    // Collect tags
    for (page_path, lpage) in &rev.pages {
        for tag in &lpage.tags {
            rev.tags
                .entry(tag.clone())
                .or_default()
                .push(page_path.clone());
        }
    }

    // Collect children
    {
        let all_input_paths = rev.pages.keys().cloned().collect::<Vec<_>>();
        for page_path in all_input_paths {
            let page_route = page_path.to_route_path();
            if let Some(parent_route) = page_route.parent() {
                if let Some(parent_path) = rev.page_routes.get(parent_route) {
                    // Arc::get_mut is _bad_
                    if let Some(parent_page) = Arc::get_mut(rev.pages.get_mut(parent_path).unwrap())
                    {
                        parent_page.children.push(page_path.clone());
                    }
                }
            }
        }
    }

    // Index pages for search
    let mut indexer = libsearch::load().indexer();

    let before_index = Instant::now();
    for (path, page) in &rev.pages {
        if page.is_indexed() {
            indexer.insert(path.clone(), page);
        }
    }
    tracing::debug!(
        "Indexed {} pages in {:?}",
        rev.pages.len(),
        before_index.elapsed()
    );

    let before_commit = Instant::now();
    let index = indexer.commit();
    tracing::debug!("Committed search index in {:?}", before_commit.elapsed());

    Ok(IndexedRevision {
        rev: Arc::new(rev),
        index: Arc::<dyn Index>::from(index),
        templates,
    })
}

fn recompute_asset_routes(rev: &mut Revision) -> eyre::Result<()> {
    for (route, asset) in &rev.assets {
        if let Asset::Derivation(derivation) = asset {
            rev.asset_routes
                .insert(derivation.input.clone(), route.clone());
        }
    }
    Ok(())
}

fn load_single_page(
    rev: Arc<Revision>,
    templates: &dyn TemplateCollection,
    page: &Page,
    mod_markdown: &'static dyn libmarkdown::Mod,
    web: WebConfig,
) -> eyre::Result<LoadedPage> {
    let path = &page.path;
    let route_path = path.to_route_path();

    tracing::debug!("Building \x1b[32m{path:?}\x1b[0m => route \x1b[36m{route_path:?}\x1b[0m");
    let mut html_buffer = Vec::with_capacity(page.markup.len() * 4);
    let args = ProcessMarkdownArgs {
        path: path.as_ref(),
        markdown: MarkdownRef::from_str(&page.markup),
        w: &mut html_buffer,
        rv: rev.clone(),
        ti: rev.ti.clone(),
        templates,
        web,
    };

    // TODO: do something about erroring pages, better than
    // "failing to load the revision"
    let res = mod_markdown
        .process_markdown_to_writer(args)
        .wrap_err_with(|| format!("processing markdown for {path:?}"))?;
    let mut deser = YamlDeserializer::new(res.frontmatter.as_deref().unwrap_or_default());
    let frontmatter: Frontmatter = deser
        .deserialize::<FrontmatterIn>()
        .map_err(|e| {
            eyre!(
                "yaml deser error for input path {path:?}: {e:?}\nFull YAML markup:\n{}\nError as display: {e}\n",
                res.frontmatter.as_deref().unwrap_or_default()
            )
        })?
        .into();

    let reading_time = res.reading_time;

    let thumb_path = path.canonicalize_relative_path(InputPathRef::from_str("_thumb.jxl"));
    let thumb = rev.media.get(&thumb_path).cloned();

    // maybe the parent will have a thumbnail?
    let parent_thumb_path =
        path.canonicalize_relative_path(InputPathRef::from_str("../_thumb.jxl"));
    let parent_thumb = rev.media.get(&parent_thumb_path).cloned();

    let lpage = LoadedPage {
        ti: rev.ti.clone(),
        web,
        path: path.to_owned(),
        route: route_path.to_owned(),
        kind: route_path.into(),

        html: String::from_utf8(html_buffer)?,
        plain_text: res.plain_text,
        reading_time,
        toc: res.toc,
        crates: Default::default(),       // TODO
        github_repos: Default::default(), // TODO
        links: res.links.into_iter().collect(),
        title: frontmatter.title,
        template: frontmatter.template,
        date: frontmatter.date,
        draft: frontmatter.draft,
        archive: frontmatter.archive,
        draft_code: frontmatter.draft_code,
        aliases: frontmatter.aliases,
        tags: frontmatter.tags,
        updated_at: frontmatter.updated_at,

        show_patreon_credits: frontmatter.extra.patreon,
        hide_patreon_plug: frontmatter.extra.hide_patreon,
        hide_comments: frontmatter.extra.hide_comments,
        hide_metadata: frontmatter.extra.hide_metadata,
        ongoing: frontmatter.extra.ongoing,

        // TODO: fill these in
        rust_version: None,

        series_link: None,
        parts: Default::default(),

        video_info: VideoInfo {
            dual_feature: frontmatter.extra.dual_feature,
            tube: frontmatter.extra.tube,
            youtube: frontmatter.extra.youtube,
            duration: frontmatter.extra.duration,
        },

        thumb: thumb.map(|thumb| conflux::PageThumb {
            path: thumb_path,
            media: thumb,
        }),
        parent_thumb: parent_thumb.map(|thumb| conflux::PageThumb {
            path: parent_thumb_path,
            media: thumb,
        }),

        children: Default::default(),
    };

    Ok(lpage)
}

fn mark_series_links(lpage: &mut LoadedPage) -> eyre::Result<()> {
    if lpage.kind == PageKind::SeriesPart {
        let series_index_path = lpage.route.parent().unwrap();

        let tokens: Vec<&str> = lpage.route.as_str().split('/').collect();
        let part_number = match tokens.as_slice() {
            ["", "series", _series_slug, part_str, ..] => part_str
                .strip_prefix("part-")
                .and_then(|part_str| part_str.parse::<usize>().ok())
                .map(PartNumber::new),
            _ => None,
        }
        .ok_or_else(|| {
            eyre!(
                "Invalid series part path: expected `/series/:slug/part-N`, but got '{}'",
                lpage.route
            )
        })?;

        lpage.series_link = Some(SeriesLink {
            index_route: series_index_path.to_owned(),
            part_number,
        });
    }

    Ok(())
}

pub fn lrev_make_template_collection(
    lrev: &mut Revision,
) -> eyre::Result<Arc<dyn TemplateCollection>> {
    let mut compile_args = CompileArgs::default();

    for template in lrev.pak.templates.values() {
        let name = match template
            .path
            .as_str()
            .strip_suffix(".jinja")
            .and_then(|s| s.strip_prefix("/templates/"))
        {
            Some(name) => name,
            None => {
                warn!(
                    "Ignoring template {:?} (no /templates/ prefix, no .jinja suffix)",
                    template.path
                );
                continue;
            }
        };

        debug!("Found template \x1b[33m{}\x1b[0m", name);
        compile_args
            .templates
            .insert(name.to_string(), template.markup.clone());
    }

    tracing::trace!("{} templates total", compile_args.templates.len());

    let modtpl = libtemplate::load();
    let coll = modtpl.make_collection(compile_args)?;
    Ok(Arc::<dyn TemplateCollection>::from(coll))
}
