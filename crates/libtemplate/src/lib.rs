use std::{
    collections::{HashMap, HashSet},
    fmt,
    ops::Deref,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};

mod conversions;
mod global_functions_and_filters;
mod impls;
mod prettify_minijinja_errors;

use closest::{GetOrHelp, ResourceKind};
use config_types::{TenantInfo, WebConfig, is_production};
use conflux::{
    AccessOverride, InputPath, LoadedPage, Media, OffsetDateTime, PageKind, RevisionView, RouteRef,
    SearchResult, SearchResults, Viewer,
};
use credentials::UserInfo;
use eyre::eyre;
use global_functions_and_filters::truncate_core;
use image_types::{ICodec, LogicalPixels};
use itertools::Itertools;
use libmedia::MediaMarkupOpts;
use libsearch::Index;
use merde::time::Rfc3339;
use minijinja::{
    Environment,
    value::{Kwargs, Object, Value},
};
use mom_types::GlobalStateView;
use prettify_minijinja_errors::PrettifyExt;
use rand::seq::SliceRandom;
use template_types::{DataObject, DataValue, GlobalsVal};

use crate::conversions::ToMinijinaResult;
use crate::global_functions_and_filters::{get_globals, get_revision_view};

#[derive(Default)]
struct ModImpl;

#[dylo::export]
impl Mod for ModImpl {
    fn make_collection(&self, args: CompileArgs) -> eyre::Result<Box<dyn TemplateCollection>> {
        let mut environment = Environment::new();

        environment.set_debug(true);

        environment.set_loader(move |path| {
            Ok(Some(
                args.templates
                    .get_or_help(ResourceKind::Template, path)
                    .map_err(|e| {
                        eprintln!("on template lookup: {e}");
                        minijinja::Error::new(minijinja::ErrorKind::TemplateNotFound, e.to_string())
                    })?
                    .clone(),
            ))
        });

        global_functions_and_filters::register_all(&mut environment);

        Ok(Box::new(TemplateCollectionImpl { environment }))
    }
}

#[cfg(feature = "impl")]
struct TemplateCollectionImpl {
    environment: Environment<'static>,
}

#[dylo::export]
impl TemplateCollection for TemplateCollectionImpl {
    fn render_template_to(
        &self,
        w: &mut dyn std::io::Write,
        args: RenderTemplateArgs<'_>,
    ) -> eyre::Result<()> {
        let template_name = args.template_name;
        let template = self
            .environment
            .get_template(template_name)
            .prettify_minijinja_error()?;

        let globals = GlobalsVal {
            page: args.page.clone(),
            user_info: args.user_info,
            additional_globals: args.additional_globals,
            raw_query: args.raw_query.to_owned(),
            url_params: form_urlencoded::parse(args.raw_query.as_bytes())
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect(),
            rv: args.rv,
            gv: args.gv,
            index: args.index,
            web: args.web,
        };

        template
            .render_to_write(Value::from_object(globals), w)
            .prettify_minijinja_error()?;
        Ok(())
    }

    fn render_shortcode_to(
        &self,
        w: &mut dyn std::io::Write,
        sc: Shortcode<'_>,
        rv: Arc<dyn RevisionView>,
        web: WebConfig,
    ) -> eyre::Result<RenderShortcodeResult> {
        let template_name = format!("shortcodes/{}.html", sc.name);
        let template_input_path = InputPath::new(format!("/templates/{template_name}.jinja"));
        let template_input = rv.rev().bs()?.inputs().get(&template_input_path).cloned().ok_or_else(|| {
                eyre!("shortcode template not found: {template_name}, tried input path {template_input_path}")
            })?;

        let cachebusted_deps: Arc<Mutex<HashSet<InputPath>>> = Arc::new(Mutex::new(HashSet::new()));
        let rv = Arc::new(impls::TrackingRevisionView::new(
            rv,
            cachebusted_deps.clone(),
        ));

        let mut args: HashMap<String, Value> = sc
            .args
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    match v {
                        DataValue::String(s) => Value::from(s),
                        DataValue::Number(n) => Value::from(n),
                        DataValue::Boolean(b) => Value::from(b),
                    },
                )
            })
            .collect();
        args.insert(
            "__revision_view".into(),
            Value::from_object(RevisionViewHolder(rv)),
        );
        args.insert("web_port".into(), web.port.into());

        if let Some(body) = sc.body {
            args.insert("body".into(), body.into());
        }

        let template = self
            .environment
            .get_template(&template_name)
            .prettify_minijinja_error()?;

        template.render_to_write(Value::from_object(args), w).bs()?;
        Ok(RenderShortcodeResult {
            shortcode_input_path: template_input.path.clone(),
            assets_looked_up: {
                let guard = cachebusted_deps.lock().unwrap();
                guard.clone()
            },
        })
    }
}

#[derive(Default)]
pub struct CompileArgs {
    pub templates: HashMap<String, String>,
}

pub struct RenderTemplateArgs<'a> {
    pub template_name: &'a str,

    /// URL structure:
    /// +-------------------------+---+-------------------+
    /// | /articles/foo           | ? | bar=baz&qux=quux  |
    /// +-------------------------+---+-------------------+
    /// | Path                    |   | Raw Query         |
    /// +-------------------------+---+-------------------+
    pub path: &'a RouteRef,
    pub raw_query: &'a str,

    /// Revision bundle
    pub rv: Arc<dyn RevisionView>,

    /// Global state view
    pub gv: Arc<dyn GlobalStateView>,

    /// Search index
    pub index: Arc<dyn Index>,

    /// Page we're rendering (optional)
    pub page: Option<Arc<LoadedPage>>,

    /// Gotten from cookies
    pub user_info: Option<UserInfo<'static>>,

    /// Web configuration
    pub web: WebConfig,

    /// Additional globals
    pub additional_globals: DataObject,
}

pub fn shortcode_name_to_input_path(name: &str) -> InputPath {
    format!("/templates/shortcodes/{name}.html.jinja").into()
}

impl TemplateCollection for () {
    fn render_template_to(
        &self,
        _w: &mut dyn std::io::Write,
        _args: RenderTemplateArgs<'_>,
    ) -> eyre::Result<()> {
        Err(eyre!("no template collection"))
    }

    fn render_shortcode_to(
        &self,
        _w: &mut dyn std::io::Write,
        _args: Shortcode<'_>,
        _rv: Arc<dyn RevisionView>,
        _web: WebConfig,
    ) -> eyre::Result<RenderShortcodeResult> {
        Err(eyre!("no template collection"))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LoadedPageVal(pub(crate) Arc<LoadedPage>);

trait AsMinijinjaValue {
    fn mj(self) -> Value;
}

impl AsMinijinjaValue for Rfc3339<OffsetDateTime> {
    fn mj(self) -> Value {
        Value::from_safe_string(
            self.0
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap(),
        )
    }
}

impl Deref for LoadedPageVal {
    type Target = LoadedPage;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl LoadedPageVal {
    pub(crate) fn get_listing(
        &self,
        globals: &GlobalsVal,
        page_number: Option<u64>,
        per_page: Option<u64>,
    ) -> eyre::Result<Listing> {
        let viewer = globals.viewer();

        let page = page_number.unwrap_or(1) as usize;
        let zero_indexed_page_number = page
            .checked_sub(1)
            .ok_or_else(|| eyre::eyre!("page out of range: must be >= 1"))?;

        let per_page = per_page.unwrap_or(u64::MAX) as usize;

        let filter: &dyn Fn(&LoadedPage) -> bool;
        let prefix = self.route.as_str();
        let parts_filter =
            move |p: &LoadedPage| p.route.as_str().starts_with(prefix) && p.series_link.is_some();

        let listing_kind = match self.kind {
            PageKind::ArticleListing => {
                filter = &|p| p.kind == PageKind::Article;
                ListingKind::Articles
            }
            PageKind::EpisodesListing => {
                filter = &|p| p.kind == PageKind::Episode;
                ListingKind::Episodes
            }
            PageKind::SeriesListing => {
                filter = &|p| p.kind == PageKind::SeriesIndex;
                ListingKind::Series
            }
            PageKind::SeriesIndex => {
                filter = &parts_filter;
                ListingKind::SeriesParts
            }
            _ => eyre::bail!("Not a listing page"),
        };

        let rv = globals.rv.as_ref();
        let pages = rv
            .rev()?
            .pages
            .values()
            .filter(|p| p.is_listed(&viewer))
            .filter(|p| filter(p.as_ref()))
            .cloned();

        let pages = match listing_kind {
            ListingKind::Articles | ListingKind::Episodes | ListingKind::Series => {
                // most recent first
                pages.sorted_by_key(|p| std::cmp::Reverse(p.date))
            }
            ListingKind::SeriesParts => {
                // oldest first
                pages.sorted_by_key(|p| p.date)
            }
        };

        let mut pages = pages
            .into_iter()
            .skip(zero_indexed_page_number * per_page)
            .take(per_page + 1)
            .map(LoadedPageVal)
            .collect::<Vec<_>>();

        let has_more = pages.len() > per_page;
        if has_more {
            pages.pop();
        }

        Ok(Listing {
            kind: listing_kind,
            items: pages,
            per_page,
            page_number: page,
            has_more,
        })
    }

    pub(crate) fn get_children(&self, globals: &GlobalsVal) -> Vec<LoadedPageVal> {
        let rev = globals.rv.rev().unwrap();
        self.children
            .iter()
            .filter_map(|path| rev.pages.get(path))
            .map(|child| LoadedPageVal(child.clone()))
            .collect()
    }
}

impl Object for LoadedPageVal {
    fn is_true(self: &Arc<Self>) -> bool {
        true
    }

    fn render(self: &Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        Some(match key.as_str()? {
            "path" => self.path.clone().into(),
            "route" => self.route.clone().into(),
            "thumb" => Value::from(self.thumb.clone().map(|m| MediaVal {
                path: m.path,
                media: m.media,
                web: self.web,
            })),
            "parent_thumb" => Value::from(self.parent_thumb.clone().map(|m| MediaVal {
                path: m.path,
                media: m.media,
                web: self.web,
            })),

            "plain_text" => self.plain_text.clone().into(),
            "short_desc" => {
                let truncated = truncate_core(self.plain_text.as_str(), 200);
                // double quotes would break the og:description meta tag, so we replace them with single quotes
                let s = truncated.replace('"', "'");
                Value::from_safe_string(s)
            }
            "html" => Value::from_safe_string(self.html.clone()),
            "html_until_more" => {
                if let Some(i) = self.html.find("<!-- more -->") {
                    Value::from_safe_string(self.html[..i].to_string())
                } else {
                    let truncated = htmlrewrite::load().truncate_html(&self.html, 400);
                    Value::from_safe_string(truncated)
                }
            }
            "html_until_playwall" => {
                if let Some(i) = self.html.find("<!-- playwall -->") {
                    Value::from_safe_string(self.html[..i].to_string())
                } else {
                    let truncated = htmlrewrite::load().truncate_html(&self.html, 400);
                    Value::from_safe_string(truncated)
                }
            }
            "reading_time" => self.reading_time.into(),
            "toc" => Value::from_serialize(&self.toc),
            "crates" => Value::from_serialize(&self.crates),
            "github_repos" => Value::from_serialize(&self.github_repos),
            "links" => Value::from_serialize(&self.links),
            "title" => self.title.clone().into(),
            "date" => self.date.mj(),
            "draft" => self.draft.into(),
            "archive" => self.archive.into(),
            "aliases" => Value::from_serialize(&self.aliases),
            "tags" => Value::from_serialize(&self.tags),
            "draft_code" => self.draft_code.clone().into(),
            "updated_at" => self.updated_at?.mj(),
            "rust_version" => self.rust_version.clone()?.into(),
            "show_patreon_credits" => self.show_patreon_credits.into(),
            "hide_patreon_plug" => self.hide_patreon_plug.into(),
            "hide_comments" => self.hide_comments.into(),
            "hide_metadata" => self.hide_metadata.into(),
            "series_link" => self.series_link.clone().map(Value::from_serialize).into(),
            "parts" => Value::from_serialize(&self.parts),

            "created_or_updated_at" => self.updated_at.unwrap_or(self.date).mj(),
            "is_old" => {
                let updated_at = self.updated_at.unwrap_or(self.date);

                let now = OffsetDateTime::now_utc();
                let two_years_ago = now.replace_year(now.year() - 2).unwrap();

                if *updated_at <= two_years_ago {
                    Value::from(true)
                } else {
                    Value::from(false)
                }
            }

            // getters!
            "url" => Value::from(self.canonical_url(self.0.web)),
            "comments_page_url" => {
                let mut u =
                    RouteRef::from_str("/api/comments").to_web_url(&self.0.ti.tc, self.0.web);
                u.query_pairs_mut()
                    .append_pair("url", self.canonical_url(self.0.web).as_str())
                    .append_pair("title", &self.title);
                Value::from_safe_string(u.as_str().to_string())
            }
            "is_articles_index" => (self.kind == PageKind::ArticleListing).into(),
            "is_series_index" => (self.kind == PageKind::SeriesListing).into(),
            "is_series_parts_index" => (self.kind == PageKind::SeriesIndex).into(),

            "exclusive_until" => {
                // 6 months
                const EXCLUSIVITY_DURATION: Duration = Duration::from_secs(60 * 60 * 24 * 30 * 6);
                if self.video_info.dual_feature {
                    let unlocks_at = self.date.0 + EXCLUSIVITY_DURATION;
                    if unlocks_at > OffsetDateTime::now_utc() {
                        Rfc3339(unlocks_at).mj()
                    } else {
                        Value::from(false)
                    }
                } else {
                    Value::from(false)
                }
            }

            "video_info" => {
                let video_info = self.video_info.clone();
                Value::from_serialize(video_info)
            }

            _ => return None,
        })
    }

    fn call_method(
        self: &Arc<Self>,
        state: &minijinja::State,
        method: &str,
        args: &[minijinja::Value],
    ) -> Result<minijinja::Value, minijinja::Error> {
        match method {
            "get_listing" => {
                let kwargs = Kwargs::try_from(args.first().cloned().ok_or_else(|| {
                    minijinja::Error::new(
                        minijinja::ErrorKind::InvalidOperation,
                        "get_listing requires an argument",
                    )
                })?)?;
                let page_number = kwargs.get("page_number")?;
                let per_page = kwargs.get("per_page")?;
                kwargs.assert_all_used()?;

                match self.get_listing(get_globals(state)?.as_ref(), page_number, per_page) {
                    Ok(listing) => Ok(listing.into()),
                    Err(e) => Err(minijinja::Error::new(
                        minijinja::ErrorKind::InvalidOperation,
                        e.to_string(),
                    )),
                }
            }
            "get_children" => {
                let children = self.get_children(get_globals(state)?.as_ref());
                Ok(Value::from(children))
            }
            _ => Err(minijinja::Error::new(
                minijinja::ErrorKind::UnknownMethod,
                format!("Unknown method: {method}"),
            )),
        }
    }
}

pub(crate) trait ToVal {
    fn to_val(self: Arc<Self>) -> Value;
}

impl ToVal for LoadedPage {
    fn to_val(self: Arc<Self>) -> Value {
        LoadedPageVal::from(self).into()
    }
}

impl From<Arc<LoadedPage>> for LoadedPageVal {
    fn from(page: Arc<LoadedPage>) -> Self {
        Self(page)
    }
}

impl From<LoadedPageVal> for Value {
    fn from(val: LoadedPageVal) -> Self {
        Value::from_object(val)
    }
}

pub(crate) struct GlobalsVal {
    pub(crate) page: Option<Arc<LoadedPage>>,
    pub(crate) user_info: Option<UserInfo<'static>>,
    pub(crate) additional_globals: DataObject,
    pub(crate) raw_query: String,
    pub(crate) url_params: HashMap<String, String>,
    pub(crate) rv: Arc<dyn RevisionView>,
    pub(crate) gv: Arc<dyn GlobalStateView>,
    pub(crate) index: Arc<dyn Index>,
    pub(crate) web: WebConfig,
}

pub(crate) struct RevisionViewHolder(pub(crate) Arc<dyn RevisionView>);

impl std::fmt::Debug for RevisionViewHolder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RevisionViewHolder").finish_non_exhaustive()
    }
}

impl Object for RevisionViewHolder {
    // not implementing anything on purpose, it's just to get at it
}

impl GlobalsVal {
    pub(crate) fn viewer(&self) -> Viewer {
        let rv = self
            .rv
            .rev()
            .expect("if we're rendering a template, surely we have a revision");

        Viewer::new(
            rv.pak.rc.clone(),
            self.user_info.as_ref(),
            AccessOverride::from_raw_query(&self.raw_query),
        )
    }
}

impl fmt::Debug for GlobalsVal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GlobalsVal").finish_non_exhaustive()
    }
}

impl Object for GlobalsVal {
    fn is_true(self: &Arc<Self>) -> bool {
        true
    }

    fn render(self: &Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GlobalsVal")
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        Some(match key.as_str()? {
            "minijinja" => "yes".into(),
            "revision" => self.rv.rev().ok()?.pak.id.clone().into(),
            "page" => Value::from_object(LoadedPageVal(self.page.clone()?)),
            "env" => if is_production() {
                "production"
            } else {
                "development"
            }
            .into(),
            "url_params" => self.url_params.clone().into(),
            "user_info" => Value::from_serialize(&self.user_info),
            "viewer" => Value::from_serialize(self.viewer()),
            "config" => Value::from_object(ConfigVal {
                ti: self.gv.gsv_ti().clone(),
                web: self.web,
            }),
            "sponsors" => Value::from_serialize(self.gv.gsv_sponsors().as_ref()),
            "globals" => Value::from_dyn_object(self.clone()),
            "web_port" => self.web.port.into(),
            "__revision_view" => Value::from_object(RevisionViewHolder(self.rv.clone())),
            other => match self.additional_globals.get(other)? {
                DataValue::String(s) => s.clone().into(),
                DataValue::Number(n) => (*n).into(),
                DataValue::Boolean(b) => (*b).into(),
            },
        })
    }

    fn call_method(
        self: &Arc<Self>,
        _state: &minijinja::State<'_, '_>,
        method: &str,
        args: &[Value],
    ) -> Result<Value, minijinja::Error> {
        match method {
            "random_article" => {
                let viewer = self.viewer();

                let pages = self
                    .rv
                    .rev()
                    .map_err(|e| {
                        minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())
                    })?
                    .pages
                    .values()
                    .filter(|p| p.is_article() && p.is_listed(&viewer))
                    .filter(|p| p.tags.iter().any(|t| t == "rust"))
                    .collect::<Vec<_>>();
                let page = (*pages.choose(&mut rand::thread_rng()).ok_or_else(|| {
                    minijinja::Error::new(
                        minijinja::ErrorKind::InvalidOperation,
                        "No articles available",
                    )
                })?)
                .clone();
                Ok(page.to_val())
            }
            "get_tag_listing" => {
                let arg = args.first().cloned().ok_or_else(|| {
                    minijinja::Error::new(
                        minijinja::ErrorKind::InvalidOperation,
                        "get_listing requires an argument",
                    )
                })?;
                let kwargs = Kwargs::try_from(arg)?;

                let tag = kwargs.get::<String>("tag")?;
                let page_number = kwargs.get::<usize>("page_number").unwrap_or(1);
                let per_page = kwargs.get::<usize>("per_page").unwrap_or(25);

                let zero_indexed_page_number = page_number.checked_sub(1).ok_or_else(|| {
                    minijinja::Error::new(
                        minijinja::ErrorKind::InvalidOperation,
                        "page out of range: must be >= 1",
                    )
                })?;
                let viewer = self.viewer();

                let rev = self.rv.rev().mj()?;

                let paths = match rev.tags.get_or_help(ResourceKind::Tag, &tag) {
                    Ok(paths) => paths,
                    Err(_e) => {
                        // return an empty listing
                        return Ok(Value::from(Listing {
                            kind: ListingKind::Articles,
                            items: Default::default(),
                            page_number,
                            per_page,
                            has_more: false,
                        }));
                    }
                };
                let mut pages = paths
                    .iter()
                    .filter_map(|p| rev.pages.get(p))
                    .filter(|p| p.is_listed(&viewer))
                    .sorted_by_key(|p| std::cmp::Reverse(p.date))
                    .skip(zero_indexed_page_number * per_page)
                    .take(per_page + 1)
                    .cloned()
                    .map(LoadedPageVal)
                    .collect::<Vec<_>>();
                let has_more = pages.len() > per_page;
                if has_more {
                    pages.pop();
                }
                Ok(Value::from(Listing {
                    kind: ListingKind::Articles,
                    items: pages,
                    page_number,
                    per_page,
                    has_more,
                }))
            }
            "search_page" => {
                let arg = args.first().cloned().ok_or_else(|| {
                    minijinja::Error::new(
                        minijinja::ErrorKind::InvalidOperation,
                        "search_page requires an argument",
                    )
                })?;
                let kwargs = Kwargs::try_from(arg)?;
                let query = kwargs.get::<String>("query")?;
                let per_page = kwargs.get::<usize>("per_page")?;
                let page_number = kwargs.get::<usize>("page_number")?;
                kwargs.assert_all_used()?;

                let viewer = self.viewer();

                let results =
                    self.index
                        .search(self.rv.as_ref(), &viewer, &query, per_page, page_number);
                Ok(SearchResultsVal(results).into())
            }
            _ => Err(minijinja::Error::new(
                minijinja::ErrorKind::UnknownMethod,
                format!("Unknown method: {method}"),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ListingKind {
    Articles,
    Episodes,
    Series,
    SeriesParts,
}

impl ListingKind {
    pub(crate) fn as_kebab_case(&self) -> &'static str {
        match self {
            ListingKind::Articles => "articles",
            ListingKind::Episodes => "episodes",
            ListingKind::Series => "series",
            ListingKind::SeriesParts => "series-parts",
        }
    }
}

#[derive(Debug)]
pub(crate) struct Listing {
    pub(crate) kind: ListingKind,
    pub(crate) items: Vec<LoadedPageVal>,
    pub(crate) page_number: usize,
    pub(crate) per_page: usize,
    pub(crate) has_more: bool,
}

impl Object for Listing {
    fn is_true(self: &Arc<Self>) -> bool {
        true
    }

    fn render(self: &Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Listing {{ kind: {:?}, items: {:?} }}",
            self.kind, self.items
        )
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        Some(match key.as_str()? {
            "kind" => self.kind.as_kebab_case().into(),
            "items" => self.items.clone().into(),
            "page_number" => self.page_number.into(),
            "per_page" => self.per_page.into(),
            "has_more" => self.has_more.into(),
            _ => return None,
        })
    }
}

impl From<Listing> for Value {
    fn from(listing: Listing) -> Self {
        Value::from_object(listing)
    }
}

pub(crate) struct ConfigVal {
    ti: Arc<TenantInfo>,
    web: WebConfig,
}
impl std::fmt::Debug for ConfigVal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfigVal").finish_non_exhaustive()
    }
}

impl Object for ConfigVal {
    fn is_true(self: &Arc<Self>) -> bool {
        true
    }

    fn render(self: &Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ConfigVal")
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        Some(match key.as_str()? {
            "web_base_url" => Value::from_safe_string(self.ti.tc.web_base_url(self.web)),
            "cdn_base_url" => Value::from_safe_string(self.ti.tc.cdn_base_url(self.web)),
            "is_development" => Value::from(self.web.env.is_dev()),
            "is_production" => Value::from(self.web.env.is_prod()),
            _ => return None,
        })
    }
}

#[repr(transparent)]
pub(crate) struct SearchResultsVal(pub(crate) SearchResults);

impl std::fmt::Debug for SearchResultsVal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Deref for SearchResultsVal {
    type Target = SearchResults;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl minijinja::value::Object for SearchResultsVal {
    fn is_true(self: &Arc<Self>) -> bool {
        true
    }

    fn render(self: &Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SearchResults {:?}", self.0)
    }

    fn get_value(
        self: &Arc<Self>,
        key: &minijinja::value::Value,
    ) -> Option<minijinja::value::Value> {
        Some(match key.as_str()? {
            "results" => minijinja::value::Value::from_object(
                self.results
                    .iter()
                    .cloned()
                    .map(SearchResultVal)
                    .collect::<Vec<_>>(),
            ),
            "num_results" => self.num_results.into(),
            "terms" => self.terms.clone().into(),
            "has_more" => self.has_more.into(),
            _ => return None,
        })
    }
}

impl From<SearchResultsVal> for Value {
    fn from(val: SearchResultsVal) -> Self {
        Self::from_object(val)
    }
}

#[repr(transparent)]
#[derive(Clone)]
pub(crate) struct SearchResultVal(SearchResult);

impl std::fmt::Debug for SearchResultVal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<SearchResultVal> for Value {
    fn from(val: SearchResultVal) -> Self {
        Self::from_object(val)
    }
}

impl Deref for SearchResultVal {
    type Target = SearchResult;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl minijinja::value::Object for SearchResultVal {
    fn is_true(self: &Arc<Self>) -> bool {
        true
    }

    fn render(self: &Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SearchResult {:?}", self.0)
    }

    fn get_value(
        self: &Arc<Self>,
        key: &minijinja::value::Value,
    ) -> Option<minijinja::value::Value> {
        Some(match key.as_str()? {
            "page" => LoadedPageVal(self.page.clone()).into(),
            "title_snippet" => {
                minijinja::value::Value::from_safe_string(self.title_snippet.to_string())
            }
            "body_snippet" => {
                minijinja::value::Value::from_safe_string(self.body_snippet.to_string())
            }
            "fragments" => minijinja::value::Value::from_safe_string(self.fragments.clone()),
            _ => return None,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MediaVal {
    pub path: InputPath,
    pub media: Media,
    pub web: WebConfig,
}

impl From<MediaVal> for Value {
    fn from(val: MediaVal) -> Self {
        Value::from_object(val)
    }
}

impl Object for MediaVal {
    fn is_true(self: &Arc<Self>) -> bool {
        true
    }

    fn render(self: &Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(media {} / {:?})", self.path, self.media)
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        Some(match key.as_str()? {
            "width" => u32::from(self.media.props.dims.w).into(),
            "height" => u32::from(self.media.props.dims.h).into(),
            _ => return None,
        })
    }

    fn call_method(
        self: &Arc<Self>,
        state: &minijinja::State,
        method: &str,
        args: &[Value],
    ) -> Result<Value, minijinja::Error> {
        match method {
            "markup" => {
                let rv = get_revision_view(state);
                let mut opts = MediaMarkupOpts {
                    path: &self.path,
                    media: &self.media,
                    rv: rv.as_ref(),
                    width: None,
                    height: None,
                    title: None,
                    alt: None,
                    id: None,
                    class: None,
                    web: self.web,
                };

                let title: Option<String>;
                let alt: Option<String>;
                let id: Option<String>;
                let class: Option<String>;

                if let Some(arg) = args.first() {
                    let kwargs = Kwargs::try_from(arg.clone())?;

                    if let Ok(width) = kwargs.get::<Value>("width") {
                        if width.is_true() {
                            if let Ok(width_num) = f32::try_from(width.clone()) {
                                opts.width = Some(LogicalPixels::from(width_num));
                            } else if let Some(width_str) = width.as_str() {
                                opts.width = Some(LogicalPixels::from(width_str.parse::<f32>().map_err(|_| {
                                    minijinja::Error::new(
                                        minijinja::ErrorKind::InvalidOperation,
                                        format!("Width must be a number or string that parses to a number, got: {:?}", width),
                                    )
                                })?));
                            }
                        }
                    }

                    if let Ok(height) = kwargs.get::<Value>("height") {
                        if height.is_true() {
                            if let Ok(height_num) = f32::try_from(height.clone()) {
                                opts.height = Some(LogicalPixels::from(height_num));
                            } else if let Some(height_str) = height.as_str() {
                                opts.height = Some(LogicalPixels::from(height_str.parse::<f32>().map_err(|_| {
                                    minijinja::Error::new(
                                        minijinja::ErrorKind::InvalidOperation,
                                        format!("Height must be a number or string that parses to a number, got: {:?}", height),
                                    )
                                })?));
                            }
                        }
                    }

                    if kwargs.has("title") {
                        title = kwargs.get::<String>("title").ok();
                        if let Some(t) = title.as_deref() {
                            opts.title = Some(t);
                        }
                    }

                    if kwargs.has("alt") {
                        alt = kwargs.get::<String>("alt").ok();
                        if let Some(a) = alt.as_deref() {
                            opts.alt = Some(a);
                        }
                    }

                    if kwargs.has("id") {
                        id = kwargs.get::<String>("id").ok();
                        if let Some(i) = id.as_deref() {
                            opts.id = Some(i);
                        }
                    }

                    if kwargs.has("class") {
                        class = kwargs.get::<String>("class").ok();
                        if let Some(c) = class.as_deref() {
                            opts.class = Some(c);
                        }
                    }

                    kwargs.assert_all_used()?;
                }

                let markup = media::load().media_html_markup(opts).map_err(|e| {
                    minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())
                })?;
                Ok(Value::from_safe_string(markup.to_string()))
            }
            "bitmap_variant_url" => {
                let ic = args.first().cloned().ok_or_else(|| {
                    minijinja::Error::new(
                        minijinja::ErrorKind::InvalidOperation,
                        "bitmap_variant expects one argument",
                    )
                })?;
                let ic = ic.as_str().ok_or_else(|| {
                    minijinja::Error::new(
                        minijinja::ErrorKind::InvalidOperation,
                        "bitmap_variant expects a string",
                    )
                })?;
                let ic = ICodec::from_str(ic).map_err(|_| {
                    minijinja::Error::new(
                        minijinja::ErrorKind::InvalidOperation,
                        format!("bitmap_variant: unknown codec: {ic}"),
                    )
                })?;

                if let Some(second) = args.get(1).cloned() {
                    let kwargs = Kwargs::try_from(second)?;
                    kwargs.assert_all_used()?;
                }

                let bv = self.media.bv.iter().filter(|bv| bv.ic == ic)
                    .sorted_by_key(|bv| std::cmp::Reverse(bv.max_width.unwrap_or_else(|| f32::MAX.into())))
                    .next()
                    .ok_or_else(|| {
                    minijinja::Error::new(
                        minijinja::ErrorKind::InvalidOperation,
                        format!("bitmap_variant: no bitmap variant for {ic} found. Available variants: {}", self.media.bv.iter().map(|bv| bv.ic.to_string()).collect::<Vec<_>>().join(", ")),
                    )
                })?;
                let rv = get_revision_view(state);
                let rev = rv.rev().map_err(|e| {
                    minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())
                })?;
                let tc = &rev.ti.tc;
                let url = bv
                    .srcset
                    .iter()
                    .max_by(|(d1, _), (d2, _)| {
                        d1.partial_cmp(d2).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(_, r)| r)
                    .unwrap()
                    .to_cdn_url_string(tc, self.web);
                Ok(Value::from_safe_string(url.to_string()))
            }
            _ => Err(minijinja::Error::new(
                minijinja::ErrorKind::UnknownMethod,
                format!("Unknown method: {method}"),
            )),
        }
    }
}
