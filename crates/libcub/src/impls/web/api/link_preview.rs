use axum::http::StatusCode;
use conflux::InputPathRef;
use cub_types::{CubReq, CubTenant as _};
use libwebpage::WebpageInfo;
use rand::seq::SliceRandom as _;
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

use crate::impls::cub_req::CubReqImpl;
use crate::impls::global_state;
use crate::impls::reply::{IntoLegacyReply, LegacyHttpError, LegacyReply, MerdeJson};

static LINK_CACHE: LazyLock<Mutex<LinkCache>> = LazyLock::new(|| Mutex::new(LinkCache::new()));

// #[axum::debug_handler]
pub(crate) async fn serve_link_preview(
    query: axum::extract::Query<HashMap<String, String>>,
    rcx: CubReqImpl,
) -> LegacyReply {
    let href = match query.get("href") {
        Some(value) => value.to_owned(),
        None => {
            tracing::warn!("Missing href parameter, returning BAD_REQUEST");
            return LegacyHttpError::with_status(StatusCode::BAD_REQUEST, "Missing href parameter")
                .into_legacy_reply();
        }
    };

    if LINK_CACHE.lock().unwrap().negative_cache.contains(&href) {
        tracing::warn!("Href found in negative cache, returning NOT_FOUND for {href}");
        return LegacyHttpError::with_status(StatusCode::NOT_FOUND, "").into_legacy_reply();
    }

    fn add_cache_control_headers(response: &mut axum::http::Response<axum::body::Body>) {
        response.headers_mut().insert(
            http::header::CACHE_CONTROL,
            http::HeaderValue::from_static("public, max-age=3600"),
        );
    }

    if let Some(cached_info) = LINK_CACHE.lock().unwrap().get(&href) {
        tracing::info!("Cache hit for {href}, returning cached information");
        let mut response = MerdeJson(cached_info).into_legacy_reply()?;
        add_cache_control_headers(&mut response);
        return Ok(response);
    }

    // allow-listed domains
    let allow_list = HashSet::from([
        "www.youtube.com",
        "youtube.com",
        "youtu.be",
        "rust-lang.org",
        "github.com",
        "patreon.com",
        "crates.io",
        "lib.rs",
        "fasterthanli.me",
        "fasterthanli.me.snug.blog",
        "bearcove.net",
        "sdr-podcast.com",
        "jamesmunns.com",
        "apple.com",
        "en.wikipedia.org",
        "misiasart.com",
        "typeof.net",
        "brailleinstitute.org",
        "fraunces.undercase.xyz",
        "www.apple.com",
        "zed.dev",
        "www.blackmagicdesign.com",
        "www.ableton.com",
        "www.penguinrandomhouse.com",
        "bsky.app",
        "hachyderm.io",
    ]);

    let parsed_href = match url::Url::parse(&href) {
        Ok(url) => url,
        Err(e) => {
            tracing::warn!("Failed to parse href {href}: {e}");
            return LegacyHttpError::with_status(StatusCode::NOT_FOUND, "").into_legacy_reply();
        }
    };
    let href_domain = parsed_href.domain().unwrap_or_default();

    fn canonicalize_url(tr: &CubReqImpl, url: &str) -> String {
        let trimmed_url = url.trim_end_matches('/');
        if trimmed_url.starts_with('/') {
            let base_url = tr.tenant.tc().web_base_url(global_state().web);
            format!("{base_url}{trimmed_url}")
        } else {
            trimmed_url.to_string()
        }
    }
    let href = canonicalize_url(&rcx, &href);

    if allow_list.contains(&href_domain) {
        // cool, let's continue!
    } else {
        // let's check by path info

        let input_path = match query.get("input-path") {
            Some(value) => value,
            None => {
                tracing::warn!("Missing input-path parameter, returning BAD_REQUEST");
                return LegacyHttpError::with_status(
                    StatusCode::BAD_REQUEST,
                    "Missing input-path parameter",
                )
                .into_legacy_reply();
            }
        };
        tracing::info!("Getting link preview for input-path: {input_path}");
        let input_path = InputPathRef::from_str(input_path);

        let irev = rcx.tenant_ref().rev()?;
        let page = irev.rev.pages.get(input_path).ok_or_else(|| {
            tracing::warn!("No page found for input-path {input_path}, returning NOT_FOUND");
            LegacyHttpError::with_status(StatusCode::NOT_FOUND, "")
        })?;

        if !page
            .links
            .iter()
            .any(|h| canonicalize_url(&rcx, h.as_str()).as_str() == href.as_str())
        {
            tracing::warn!("Link {href} not found in page links, returning NOT_FOUND");
            if let Some(closest_link) = page.links.iter().max_by(|a, b| {
                strsim::jaro_winkler(href.as_str(), a.as_str())
                    .partial_cmp(&strsim::jaro_winkler(href.as_str(), b.as_str()))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                tracing::warn!("{href} not found, did you mean {closest_link}?");
            } else {
                tracing::warn!("{href} not found, no links in page");
            }

            LINK_CACHE.lock().unwrap().insert_negative(&href);
            return LegacyHttpError::with_status(StatusCode::NOT_FOUND, "").into_legacy_reply();
        }
    }

    tracing::info!("Fetching webpage info for {href}");
    let webpage = libwebpage::load();

    let info_future = webpage.webpage_info(href.clone());
    let info = match info_future.await {
        Ok(info) => info,
        Err(e) => {
            tracing::warn!("Error fetching webpage info for {href}: {:?}", e);
            LINK_CACHE.lock().unwrap().insert_negative(&href);
            return LegacyHttpError::with_status(StatusCode::NOT_FOUND, "").into_legacy_reply();
        }
    };

    tracing::info!("Caching info for {href}");
    LINK_CACHE
        .lock()
        .unwrap()
        .insert(href.clone(), info.clone());
    let mut response = MerdeJson(info).into_legacy_reply()?;
    add_cache_control_headers(&mut response);
    Ok(response)
}

struct LinkCache {
    positive_cache: HashMap<String, WebpageInfo>,
    negative_cache: HashSet<String>,
}

impl LinkCache {
    fn new() -> Self {
        LinkCache {
            positive_cache: HashMap::new(),
            negative_cache: HashSet::new(),
        }
    }

    fn get(&self, href: &str) -> Option<WebpageInfo> {
        self.positive_cache.get(href).cloned()
    }

    fn insert(&mut self, href: String, info: WebpageInfo) {
        self.trim_if_needed();
        self.positive_cache.insert(href, info);
    }

    fn insert_negative(&mut self, href: &str) {
        self.trim_if_needed();
        self.negative_cache.insert(href.to_string());
    }

    fn trim_if_needed(&mut self) {
        const MAX_ENTRIES: usize = 2048;
        const EVICTION_COUNT: usize = 64;

        if self.positive_cache.len() > MAX_ENTRIES {
            let mut rng = rand::thread_rng();
            let keys_to_remove: Vec<_> = self
                .positive_cache
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .choose_multiple(&mut rng, EVICTION_COUNT)
                .cloned()
                .collect();

            for key in keys_to_remove {
                self.positive_cache.remove(&key);
            }
        }

        if self.negative_cache.len() > MAX_ENTRIES {
            let mut rng = rand::thread_rng();
            let keys_to_remove: Vec<_> = self
                .negative_cache
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .choose_multiple(&mut rng, EVICTION_COUNT)
                .cloned()
                .collect();

            for key in keys_to_remove {
                self.negative_cache.remove(&key);
            }
        }
    }
}
