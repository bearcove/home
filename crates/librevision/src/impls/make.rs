use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, LazyLock},
    time::Instant,
};

use camino::{Utf8Path, Utf8PathBuf};
use config_types::{RevisionConfig, TenantInfo, WebConfig};
use conflux::{
    Dimensions, Input, InputHash, InputPath, InputPathRef, MediaKind, MediaProps, Page, Pak,
    PathMappings, ROOT_INPUT_PATHS, Revision, RevisionId, SvgFontFace, SvgFontFaceCollection,
    Template,
};
use content_type::ContentType;
use cub_types::{IndexedRevision, PathMetadata};
use eyre::Context;
use facet_pretty::FacetPretty;
use image_types::{ICodec, PixelDensity};
use itertools::Itertools;
use merde::time::Rfc3339;
use tokio::sync::{Semaphore, mpsc};
use tracing::{debug, warn};
use uffmpeg::{ffmpeg_metadata_to_media_props, gather_ffmpeg_meta};
use ulid::Ulid;

mod media_props_cache;
use media_props_cache::MediaPropsCache;

use crate::{InputEvent, RevisionKind, RevisionSpec, impls::load::load_pak};

pub static IGNORED_EXTS: LazyLock<HashSet<&str>> = LazyLock::new(|| {
    let mut set = HashSet::new();
    set.insert("bkp");
    set.insert("pcapng");
    set.insert("gz");
    set.insert("DS_Store");
    set.insert("woff");
    set.insert("ttf");
    set.insert("gitignore");
    set.insert("");
    set
});

pub async fn is_path_ignored(path: &Utf8Path) -> bool {
    is_path_ignored_with_meta(path, None).await
}

pub async fn is_path_ignored_with_meta(path: &Utf8Path, metadata: Option<&PathMetadata>) -> bool {
    let (_base, ext) = InputPathRef::from_str(path.as_str()).explode();
    IGNORED_EXTS.contains(ext)
        && match metadata {
            Some(metadata) => metadata.is_file(),
            None => match tokio::fs::metadata(path).await {
                Ok(metadata) => metadata.is_file(),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                Err(e) => {
                    warn!("Error checking metadata: {}", e);
                    false
                }
            },
        }
}

pub fn input_hash_from_contents(contents: &[u8]) -> InputHash {
    let h = seahash::hash(contents);
    InputHash::new(format!("{h:016x}"))
}

/// Make a new revision (either from scratch or iteratively)
pub async fn make_revision(
    ti: Arc<TenantInfo>,
    spec: RevisionSpec,
    web: WebConfig,
) -> eyre::Result<IndexedRevision> {
    let init_start = Instant::now();

    let RevisionSpec { mappings, kind } = spec;
    let mut prev_rev: Option<Arc<Revision>> = None;

    let rev_kind_start = Instant::now();
    let (mut pak, events): (Pak, VecDeque<InputEvent>) = match kind {
        RevisionKind::FromScratch => {
            let from_scratch_start = Instant::now();
            let pak = revision_new();

            let create_event_start = Instant::now();
            let mut events = VecDeque::new();
            for input_path in ROOT_INPUT_PATHS {
                let disk_path = mappings.to_disk_path(input_path)?;
                let metadata = tokio::fs::metadata(disk_path).await?.into();
                events.push_back(InputEvent::Created {
                    path: InputPath::from(input_path),
                    metadata,
                });
            }
            let create_event_duration = create_event_start.elapsed();
            tracing::debug!("Creating initial event took {:?}", create_event_duration);

            let from_scratch_duration = from_scratch_start.elapsed();
            tracing::debug!(
                "FromScratch revision setup took {:?}",
                from_scratch_duration
            );
            (pak, events)
        }
        RevisionKind::Wake { prev } => {
            let wake_start = Instant::now();

            let wake_events_start = Instant::now();
            let events = wake_revision_events(prev.rev.as_ref(), &ti).await?;
            let wake_events_duration = wake_events_start.elapsed();
            tracing::debug!(
                "Wake revision events generation took {:?}",
                wake_events_duration
            );

            if events.is_empty() {
                // no wake events, return previous revision
                tracing::debug!("No wake events, returning previous revision");
                return Ok(prev);
            }

            prev_rev = Some(prev.rev.clone());
            let result = (prev.rev.pak.clone(), events);

            let wake_duration = wake_start.elapsed();
            tracing::debug!("Wake revision setup took {:?}", wake_duration);
            result
        }
        RevisionKind::Incremental { prev, events } => {
            let incremental_start = Instant::now();

            prev_rev = Some(prev.clone());
            let result = (prev.pak.clone(), events);

            let incremental_duration = incremental_start.elapsed();
            tracing::debug!("Incremental revision setup took {:?}", incremental_duration);
            result
        }
    };
    let rev_kind_duration = rev_kind_start.elapsed();
    tracing::debug!("Revision kind processing took {:?}", rev_kind_duration);

    pak.id = generate_rev_id();

    let db_path = ti.internal_dir().join("media_props_cache.redb");
    let db_create_start = Instant::now();
    let mut db = redb::Database::create(db_path)?;
    let db_create_duration = db_create_start.elapsed();
    tracing::debug!("Database creation took {:?}", db_create_duration);

    let integrity_check_start = Instant::now();
    let check_integrity = db.check_integrity();
    let integrity_check_duration = integrity_check_start.elapsed();
    tracing::debug!(
        "Database integrity check took {:?}",
        integrity_check_duration
    );
    match check_integrity {
        Ok(_) => {
            tracing::debug!("Database integrity check passed");
        }
        Err(e) => match e {
            redb::DatabaseError::DatabaseAlreadyOpen => {
                tracing::warn!("Database is already open, attempting to reopen");
                // Attempt to reopen or wait and retry
                // For now, we'll just return an error
                return Err(eyre::eyre!("Database is already open"));
            }
            redb::DatabaseError::RepairAborted => {
                tracing::error!("Database repair was aborted");
                return Err(eyre::eyre!("Database repair was aborted"));
            }
            redb::DatabaseError::UpgradeRequired(version) => {
                tracing::warn!("Database upgrade required to version {}", version);
                // Here we could implement an upgrade process
                // For now, we'll just return an error
                return Err(eyre::eyre!(
                    "Database upgrade required to version {}",
                    version
                ));
            }
            redb::DatabaseError::Storage(_storage_error) => {
                tracing::error!("Database storage error occurred");
                return Err(eyre::eyre!("Database storage error"));
            }
            _ => {
                tracing::error!("Unknown database error: {:?}", e);
                return Err(eyre::eyre!("Unknown database error occurred"));
            }
        },
    }

    let begin_write_start = Instant::now();
    let wtx = db.begin_write()?;
    let begin_write_duration = begin_write_start.elapsed();
    tracing::debug!("Database begin_write took {:?}", begin_write_duration);
    let media_props_cache = Arc::new(MediaPropsCache::new(wtx));

    let mods_start = Instant::now();
    let mods = RevisionMods::default();
    let mods_duration = mods_start.elapsed();
    tracing::debug!("Loading revision mods took {:?}", mods_duration);

    let (tx_add, mut rx_add) = mpsc::channel(256);
    let mut cx = MakeContext {
        tx: tx_add,
        ti,
        pak,
        events,
        modified: Default::default(),
        concurrency_limit: Arc::new(Semaphore::new(4)),
        mods,
        mappings: mappings.clone(),
        media_props_cache,
    };
    let unique_start = Instant::now();
    cx.events = cx.events.into_iter().unique().collect();
    let unique_elapsed = unique_start.elapsed();
    tracing::debug!("Uniquifying events took {:?}", unique_elapsed);

    let mut num_events = 0;
    let mut num_add_actions = 0;

    tracing::debug!("Make revision init took {:?}", init_start.elapsed());
    let start = Instant::now();

    while let Some(ev) = cx.events.pop_front() {
        cx.process_event(ev).await?;
        num_events += 1;

        // process any add events here
        let mut num_processed = 0;
        loop {
            match rx_add.try_recv() {
                Ok(action) => {
                    num_processed += 1;
                    num_add_actions += 1;
                    action.apply(&mut cx.pak)?;
                }
                Err(e) => match e {
                    mpsc::error::TryRecvError::Empty => {
                        // that's ok
                        break;
                    }
                    mpsc::error::TryRecvError::Disconnected => {
                        unreachable!("I'm sorry what")
                    }
                },
            }
        }
        if num_processed > 0 {
            tracing::debug!("Processed {num_processed} add actions in a loop turn");
        }
    }

    // this drops tx, which means we'll stop receiving things from rx_add when all
    // the spawned tasks finish.

    let MakeContext {
        mut pak,
        ti,
        tx,
        media_props_cache,
        ..
    } = cx;
    drop(tx);

    match tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while let Some(ev) = rx_add.recv().await {
            num_add_actions += 1;
            ev.apply(&mut pak)?;
            tracing::debug!("Applied while draining, soon we'll be actually drained I guess...");
        }
        Ok::<_, eyre::Report>(())
    })
    .await
    {
        Ok(result) => result?,
        Err(_) => {
            tracing::warn!("Timeout waiting for add actions to complete");
        }
    }

    let elapsed = start.elapsed();
    tracing::info!(
        "Processed \x1b[33m{num_events}\x1b[0m events in \x1b[36m{elapsed:?}\x1b[0m (\x1b[32m{num_add_actions}\x1b[0m add actions)"
    );

    // now commit the media props cache
    let media_props_cache = Arc::try_unwrap(media_props_cache).unwrap_or_else(|_| {
        panic!("The Media props cache should not be in any more tasks by now.")
    });
    tracing::debug!("Media props cache stats: {}", media_props_cache.get_stats());

    let commit_start = Instant::now();
    media_props_cache
        .commit()
        .wrap_err("while committing media props cache")?;
    let commit_duration = commit_start.elapsed();
    tracing::debug!("Media props cache commit took {:?}", commit_duration);

    // Find the input with path "/home.json"
    let home_json_input_path = InputPath::from("/home.json");
    let home_json_input = pak.inputs.get(&home_json_input_path);
    let rc = if let Some(_input) = home_json_input {
        // Read from disk using the mappings (mappings was cloned earlier)
        let home_json_disk_path = mappings.to_disk_path(&home_json_input_path)?;
        let home_json_contents = tokio::fs::read_to_string(&home_json_disk_path)
            .await
            .wrap_err_with(|| format!("Failed to read /home.json at {home_json_disk_path}"))?;
        let rc: RevisionConfig =
            facet_json::from_str(&home_json_contents).map_err(|e| eyre::eyre!(e.to_string()))?;
        rc
    } else {
        return Err(eyre::eyre!(
            "Missing revision config: did not find /home.json in the inputs"
        ));
    };

    let font_collection_start = Instant::now();
    pak.svg_font_face_collection = Arc::new(gather_svg_font_face_collection(&ti, &rc).await?);
    let font_collection_duration = font_collection_start.elapsed();
    tracing::debug!(
        "Gathering SVG font face collection took {:?}",
        font_collection_duration
    );

    tracing::info!("Revision config: {}", rc.pretty());
    pak.rc = rc;

    let load_pak_start = Instant::now();
    let rev = load_pak(pak, ti, prev_rev.as_deref(), mappings, web).await?;
    let load_pak_duration = load_pak_start.elapsed();
    tracing::debug!("Loading pak took {:?}", load_pak_duration);
    Ok(rev)
}

struct MakeContext {
    pak: Pak,
    ti: Arc<TenantInfo>,
    events: VecDeque<InputEvent>,
    modified: HashSet<InputPath>,
    tx: mpsc::Sender<AddAction>,
    concurrency_limit: Arc<tokio::sync::Semaphore>,
    mods: RevisionMods,
    mappings: PathMappings,
    media_props_cache: Arc<MediaPropsCache>,
}

impl MakeContext {
    async fn process_event(&mut self, ev: InputEvent) -> eyre::Result<()> {
        tracing::trace!("Processing event: {ev:#?}");
        match ev {
            InputEvent::Created { path, metadata } => {
                if metadata.is_file {
                    let tx = self.tx.clone();
                    let mappings = self.mappings.clone();
                    let permit = self
                        .concurrency_limit
                        .clone()
                        .acquire_owned()
                        .await
                        .unwrap();
                    let mods = self.mods;
                    let media_props_cache = self.media_props_cache.clone();

                    // TODO: Spawning async tasks is not really what we want here, we want to use CPU, not do async I/O.
                    tokio::spawn(async move {
                        let start = Instant::now();
                        let path_copy = path.clone();
                        const TIMEOUT_DURATION_SECS: u64 = 20;
                        let res = tokio::time::timeout(
                            std::time::Duration::from_secs(TIMEOUT_DURATION_SECS),
                            revision_added(
                                &tx,
                                path,
                                metadata,
                                mappings.clone(),
                                mods,
                                media_props_cache,
                            ),
                        )
                        .await
                        .unwrap_or_else(|_| {
                            Err(eyre::eyre!(
                                "{path_copy} timed out after {TIMEOUT_DURATION_SECS} seconds"
                            ))
                        });
                        let elapsed = start.elapsed();
                        let color = if elapsed.as_millis() < 10 {
                            "\x1b[32m" // green
                        } else if elapsed.as_secs_f64() <= 1.0 {
                            "\x1b[33m" // orange
                        } else {
                            "\x1b[31m" // red
                        };
                        drop(permit);
                        tracing::debug!(
                            "Processed in {color}{:?}\x1b[0m: \x1b[33m{path_copy}\x1b[0m",
                            elapsed
                        );
                        if let Err(e) = res {
                            tracing::warn!("Revision error: {e:?}");
                            _ = tx.send(AddAction::Error(e));
                        };
                    });
                } else if metadata.is_dir() {
                    for entry in self.mappings.to_disk_path(&path)?.read_dir_utf8().unwrap() {
                        let entry = entry.unwrap();
                        let path = entry.path();
                        let metadata = PathMetadata::from(entry.metadata().unwrap());
                        if is_path_ignored_with_meta(path, Some(&metadata)).await {
                            continue;
                        }

                        self.events.push_back(InputEvent::Created {
                            path: self.mappings.to_input_path(path).unwrap(),
                            metadata,
                        });
                    }
                } else {
                    // ignore non-file/non-dir
                }
            }
            InputEvent::Modified { path, metadata } => {
                if metadata.is_file() {
                    self.modified.insert(path.clone());
                    revision_modified(
                        &self.tx,
                        &mut self.pak,
                        path.clone(),
                        metadata,
                        self.mappings.clone(),
                        self.mods,
                        self.media_props_cache.clone(),
                    )
                    .await?;
                } else if metadata.is_dir() {
                    for entry in self.mappings.to_disk_path(&path)?.read_dir_utf8().unwrap() {
                        let entry = entry.unwrap();
                        let path = entry.path();
                        let metadata = PathMetadata::from(entry.metadata().unwrap());
                        if is_path_ignored_with_meta(path, Some(&metadata)).await {
                            continue;
                        }

                        self.events.push_back(InputEvent::Modified {
                            path: self.mappings.to_input_path(path).unwrap(),
                            metadata,
                        });
                    }
                } else {
                    // ignore non-file/non-dir
                }
            }
            InputEvent::Removed(input_path) => {
                // remove any input files that _start_ with that path
                let to_remove = self
                    .pak
                    .inputs
                    .keys()
                    .filter(|p| p.as_str().starts_with(input_path.as_str()))
                    .cloned()
                    .collect::<Vec<_>>();
                tracing::debug!(
                    "Removing {} files, all that start with {}",
                    to_remove.len(),
                    input_path,
                );

                for input_path in to_remove {
                    revision_removed(&mut self.pak, input_path.as_ref()).await?;
                }
            }
            InputEvent::NewMetadata { path, metadata } => {
                // Just turn this into a remove+add pair - that's safer than trying
                // to mutate in place
                self.events.push_back(InputEvent::Removed(path.clone()));
                self.events.push_back(InputEvent::Created {
                    path: path.clone(),
                    metadata,
                });
            }
        }

        Ok(())
    }
}

#[derive(Clone, Copy)]
struct RevisionMods {
    image: &'static dyn libimage::Mod,
    svg: &'static dyn libsvg::Mod,
}

impl Default for RevisionMods {
    fn default() -> Self {
        RevisionMods {
            image: libimage::load(),
            svg: libsvg::load(),
        }
    }
}

async fn gather_svg_font_face_collection(
    ti: &TenantInfo,
    rc: &RevisionConfig,
) -> eyre::Result<SvgFontFaceCollection> {
    let mut coll = SvgFontFaceCollection::default();
    for spec in &rc.svg_fonts {
        let absolute_path = ti.base_dir.join(&spec.path);
        if tokio::fs::metadata(&absolute_path).await.is_err() {
            return Err(eyre::eyre!(
                "Font file not found: {absolute_path} (specified in tenant config as {}, should be relative to base dir {})",
                &spec.path,
                ti.base_dir
            ));
        }

        let contents = tokio::fs::read(&absolute_path)
            .await
            .wrap_err_with(|| format!("while reading font file {absolute_path}"))?;
        let hash = input_hash_from_contents(&contents);

        coll.faces.push(SvgFontFace {
            family: spec.family.clone(),
            weight: spec.weight,
            style: spec.style,
            hash,
            contents,
            file_name: absolute_path
                .file_name()
                .ok_or_else(|| eyre::eyre!("Path has no file name: {absolute_path}"))?
                .to_string(),
        });
    }
    Ok(coll)
}

pub(crate) enum AddAction {
    InsertInput { path: InputPath, input: Input },
    InsertPage { path: InputPath, page: Page },
    InsertMediaProps { path: InputPath, props: MediaProps },
    InsertTemplate { path: InputPath, template: Template },
    Error(eyre::Report),
}

impl AddAction {
    fn apply(self, pak: &mut Pak) -> eyre::Result<()> {
        match self {
            AddAction::InsertInput { path, input } => {
                pak.inputs.insert(path, input);
            }
            AddAction::InsertPage { path, page } => {
                pak.pages.insert(path, page);
            }
            AddAction::InsertMediaProps { path, props: media } => {
                pak.media_props.insert(path, media);
            }
            AddAction::InsertTemplate { path, template } => {
                pak.templates.insert(path, template);
            }
            AddAction::Error(e) => {
                return Err(e);
            }
        }
        Ok(())
    }
}

/// Signals this Revision that a file has been added
async fn revision_added(
    tx: &mpsc::Sender<AddAction>,
    path: InputPath,
    metadata: PathMetadata,
    mappings: PathMappings,
    mods: RevisionMods,
    media_props_cache: Arc<MediaPropsCache>,
) -> eyre::Result<()> {
    let disk_path = mappings.to_disk_path(&path)?;
    if is_path_ignored(&disk_path).await {
        return Ok(());
    }
    let contents = tokio::fs::read(&disk_path).await?;
    let hash = input_hash_from_contents(&contents);
    let mtime: time::OffsetDateTime = metadata.mtime;

    let content_type =
        ContentType::guess_from_path(path.as_str()).unwrap_or(ContentType::OctetStream);
    tracing::debug!(
        "Added: \x1b[35m{content_type}\x1b[0m \x1b[33m{path}\x1b[0m~\x1b[36m{hash}\x1b[0m (last mod \x1b[32m{mtime}\x1b[0m)"
    );

    tx.send(AddAction::InsertInput {
        path: path.to_owned(),
        input: Input {
            hash: hash.clone(),
            path: path.to_owned(),
            mtime: Rfc3339(mtime),
            size: metadata.len,
            content_type,
        },
    })
    .await?;

    match content_type {
        ContentType::Markdown => {
            // Do not collect dependencies at this stage because we need the full template
            // collection to be able to track media and other asset lookups through shortcodes.

            tx.send(AddAction::InsertPage {
                path: path.to_owned(),
                page: Page {
                    hash,
                    path: path.to_owned(),
                    markup: String::from_utf8(contents)?,
                    deps: Default::default(),
                },
            })
            .await?;
        }
        ContentType::SCSS => {
            return Err(eyre::eyre!(
                "SCSS file found in content directory: {}. SCSS should be imported from src/bundle.ts. There should be no SCSS files in the content directory.",
                disk_path
            ));
        }
        ContentType::JXL | ContentType::PNG => {
            let codec = match content_type {
                ContentType::JXL => ICodec::JXL,
                ContentType::PNG => ICodec::PNG,
                _ => unreachable!(),
            };

            let props = media_props_cache
                .get_or_insert_with(&hash, async || {
                    let (w, h) = mods.image.dimensions(&contents, codec)?;

                    // only @2x images are supported right now (or 1x, with no suffix) — no @3x, etc.
                    let density = if path.as_str().contains("@2x.") {
                        PixelDensity::TWO
                    } else {
                        PixelDensity::ONE
                    };
                    let dimensions = Dimensions { w, h, density };

                    let mut props = MediaProps::new(MediaKind::Bitmap, dimensions, 1.0);
                    props.ic = Some(codec);

                    Ok(props)
                })
                .await?;

            tx.send(AddAction::InsertMediaProps {
                path: path.to_owned(),
                props,
            })
            .await?;
        }
        ContentType::JPG | ContentType::WEBP | ContentType::AVIF => {
            debug!("This should be a JXL: {path}");
        }
        ContentType::DrawIO => {
            let props = media_props_cache
                .get_or_insert_with(&hash, async || {
                    let svg_bytes = mods
                        .svg
                        .drawio_to_svg(
                            contents.into(),
                            libsvg::DrawioToSvgOptions { minify: false },
                        )
                        .await
                        .wrap_err("converting drawio to SVG to get its dimensions")?;
                    let svg_dimensions = mods
                        .svg
                        .svg_dimensions(&svg_bytes[..])
                        .ok_or_else(|| eyre::eyre!("SVG did not have dimensions"))?;

                    Ok(MediaProps::new(MediaKind::Diagram, svg_dimensions, 1.0))
                })
                .await?;

            tx.send(AddAction::InsertMediaProps {
                path: path.to_owned(),
                props,
            })
            .await?;
        }
        ContentType::SVG => {
            let props = media_props_cache
                .get_or_insert_with(&hash, async || {
                    let dimensions = mods
                        .svg
                        .svg_dimensions(&contents[..])
                        .expect("SVG did not have dimensions");

                    Ok(MediaProps::new(MediaKind::Diagram, dimensions, 1.0))
                })
                .await?;

            tx.send(AddAction::InsertMediaProps {
                path: path.to_owned(),
                props,
            })
            .await?;
        }
        ContentType::MP4 => {
            let props = media_props_cache
                .get_or_insert_with(&hash, async || {
                    let mut props = ffmpeg_metadata_to_media_props(
                        gather_ffmpeg_meta(disk_path.clone())
                            .await
                            .wrap_err(format!("while gathering metadata from {disk_path}"))?,
                    );

                    let (base, _ext) = path.explode();
                    if base.ends_with("@2x") {
                        props.dims.density = PixelDensity::TWO;
                    }

                    Ok(props)
                })
                .await?;

            tx.send(AddAction::InsertMediaProps {
                path: path.to_owned(),
                props,
            })
            .await?;
        }
        ContentType::Jinja => {
            tx.send(AddAction::InsertTemplate {
                path: path.to_owned(),
                template: Template {
                    path: path.to_owned(),
                    markup: String::from_utf8(contents)?,
                },
            })
            .await?;
        }
        _ => {
            // ignore others
        }
    }

    Ok(())
}

/// Signals this Revision that a file has been modified
async fn revision_modified(
    tx: &mpsc::Sender<AddAction>,
    revision: &mut Pak,
    path: InputPath,
    metadata: PathMetadata,
    mappings: PathMappings,
    mods: RevisionMods,
    media_props_cache: Arc<MediaPropsCache>,
) -> eyre::Result<()> {
    // for now, just do removed+added.
    // later, compare modtime, size, etc.

    revision_removed(revision, &path).await?;
    revision_added(tx, path, metadata, mappings, mods, media_props_cache).await?;

    Ok(())
}

/// Signals this Revision that a file has been removed
pub(crate) async fn revision_removed(revision: &mut Pak, path: &InputPathRef) -> eyre::Result<()> {
    revision.inputs.remove(path);
    revision.pages.remove(path);
    revision.templates.remove(path);
    revision.media_props.remove(path);

    Ok(())
}

/// Create a new empty revision
pub fn revision_new() -> Pak {
    Pak {
        id: generate_rev_id(),
        inputs: Default::default(),
        pages: Default::default(),
        templates: Default::default(),
        media_props: Default::default(),
        svg_font_face_collection: Default::default(),
        rc: Default::default(),
    }
}

/// Generate `SingleEvent` to update a revision from what it thinks it is vs
/// what is actually on disk.
pub async fn wake_revision_events(
    prev_lrev: &Revision,
    ti: &TenantInfo,
) -> eyre::Result<VecDeque<InputEvent>> {
    let mappings = PathMappings::from_ti(ti);
    let mut events = VecDeque::new();

    // Collect all inputs in ROOT_INPUT_PATHS recursively
    let mut current_inputs: HashMap<InputPath, PathMetadata> = HashMap::new();
    let mut dirs = ROOT_INPUT_PATHS
        .iter()
        .map(|&path| mappings.to_disk_path(path).unwrap())
        .collect::<Vec<_>>();
    while let Some(dir) = dirs.pop() {
        let mut read_dir = tokio::fs::read_dir(dir).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            let path = Utf8PathBuf::from_path_buf(entry.path()).unwrap();
            let metadata = PathMetadata::from(entry.metadata().await?);
            if is_path_ignored_with_meta(&path, Some(&metadata)).await {
                continue;
            }

            if metadata.is_dir() {
                dirs.push(path);
            } else if metadata.is_file() {
                current_inputs.insert(mappings.to_input_path(&path)?, metadata);
            }
        }
    }

    for (input_path, metadata) in &current_inputs {
        if let Some(prev_input) = prev_lrev.pak.inputs.get(input_path) {
            enum Outcome {
                Unchanged,
                MetadataChanged,
                ContentsChanged,
            }

            let modtime: time::OffsetDateTime = metadata.mtime;
            let meta_changed = if prev_input.size != metadata.len {
                tracing::debug!("File size changed: {input_path}");
                true
            } else if prev_input.mtime.0 != modtime {
                tracing::debug!("File modtime changed: {input_path}");
                true
            } else {
                // same everything
                false
            };

            let outcome = if meta_changed {
                // maybe the contents changed too?
                let contents = tokio::fs::read(mappings.to_disk_path(input_path)?).await?;
                let hash = input_hash_from_contents(&contents);
                if hash != prev_input.hash {
                    tracing::debug!("File modtime + hash changed: {input_path}");
                    Outcome::ContentsChanged
                } else {
                    // metadata changed but hash is the same
                    Outcome::MetadataChanged
                }
            } else {
                Outcome::Unchanged
            };

            match outcome {
                Outcome::ContentsChanged => {
                    events.push_back(InputEvent::Modified {
                        path: input_path.clone(),
                        metadata: metadata.clone(),
                    });
                }
                Outcome::MetadataChanged => {
                    events.push_back(InputEvent::NewMetadata {
                        path: input_path.clone(),
                        metadata: metadata.clone(),
                    });
                }
                Outcome::Unchanged => {
                    // do nothing
                }
            }
        } else {
            events.push_back(InputEvent::Created {
                path: input_path.clone(),
                metadata: metadata.clone(),
            });
        }
    }

    // Check for removed files
    for prev_input_path in prev_lrev.pak.inputs.keys() {
        if !current_inputs.contains_key(prev_input_path) {
            events.push_back(InputEvent::Removed(prev_input_path.clone()));
        }
    }
    Ok(events)
}

/// Generate a new revision ID
pub fn generate_rev_id() -> RevisionId {
    RevisionId::new(format!("rev_{}", Ulid::new()))
}
