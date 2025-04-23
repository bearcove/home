use conflux::{CacheBuster, InputPath, RevisionView};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

// This wraps another type.implements revision view, but it tracks all calls to asset URL and every
// successful call is added as a dependency to some list.
pub struct TrackingRevisionView {
    pub tracking_cachebuster: TrackingCacheBuster,
    pub rv: Arc<dyn RevisionView>,
}

impl TrackingRevisionView {
    pub fn new(
        rv: Arc<dyn RevisionView>,
        cachebusted_deps: Arc<Mutex<HashSet<InputPath>>>,
    ) -> Self {
        Self {
            tracking_cachebuster: TrackingCacheBuster::new(rv.clone(), cachebusted_deps),
            rv,
        }
    }
}

impl RevisionView for TrackingRevisionView {
    fn rev(&self) -> conflux::Result<&conflux::Revision, conflux::RevisionError> {
        self.rv.rev()
    }

    fn cachebuster(&self) -> &dyn conflux::CacheBuster {
        &self.tracking_cachebuster
    }
}

pub struct TrackingCacheBuster {
    pub rv: Arc<dyn RevisionView>,
    pub deps: Arc<Mutex<HashSet<InputPath>>>,
}

impl TrackingCacheBuster {
    pub fn new(rv: Arc<dyn RevisionView>, deps: Arc<Mutex<HashSet<InputPath>>>) -> Self {
        Self { rv, deps }
    }
}

impl CacheBuster for TrackingCacheBuster {
    fn asset_url(
        &self,
        web: config::WebConfig,
        asset_path: &conflux::InputPathRef,
    ) -> conflux::Result<conflux::AbsoluteUrl> {
        self.deps.lock().unwrap().insert(asset_path.to_owned());
        let inner_cb = self.rv.cachebuster();
        inner_cb.asset_url(web, asset_path)
    }

    fn media(&self, path: &conflux::InputPathRef) -> conflux::Result<&conflux::Media> {
        self.deps.lock().unwrap().insert(path.to_owned());
        self.rv.cachebuster().media(path)
    }
}
