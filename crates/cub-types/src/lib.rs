use mom_types::Sponsors;
use std::{collections::HashMap, sync::Arc};
use time::OffsetDateTime;

use config_types::{RedditSecrets, RevisionConfig, TenantConfig, TenantInfo, WebConfig};
use conflux::{Revision, RevisionError, RouteRef};
use futures_core::future::BoxFuture;
use hattip::{
    HReply,
    http::{Uri, request::Parts},
};
use libmomclient::MomTenantClient;
use libobjectstore::ObjectStore;
use libsearch::Index;
use libwebsock::WebSocketStream;
use template_types::TemplateCollection;

/// An indexed revision (with a template collection, a search index, etc.)
#[derive(Clone)]
pub struct IndexedRevision {
    pub rev: Arc<Revision>,
    pub index: Arc<dyn Index>,
    pub templates: Arc<dyn TemplateCollection>,
}

pub trait CubReq: Send + Sync + 'static {
    /// Returns the web config
    fn web(&self) -> WebConfig;

    /// Returns the request path, eg. `/articles/i-like-routing`
    fn route(&self) -> &RouteRef;

    /// Returns the request URI
    fn uri(&self) -> &Uri;

    /// Returns the request parts
    fn parts(&self) -> &Parts;

    /// Returns the query string, parsed
    fn url_params(&self) -> Vec<(String, String)>;

    /// Returns the query string, parsed as a HashMap
    fn url_params_map(&self) -> HashMap<String, String> {
        self.url_params().into_iter().collect()
    }

    /// Borrows the tenant
    fn tenant_ref(&self) -> &dyn CubTenant;

    /// Clones a handle the tenant
    fn tenant_owned(&self) -> Arc<dyn CubTenant>;

    /// Returns reddit secrets (if any)
    fn reddit_secrets(&self) -> eyre::Result<&RedditSecrets>;

    /// Returns true if the request has a websocket upgrade
    fn has_ws(&self) -> bool;

    /// Performs a websocket upgrade, calls the provided callback
    fn on_ws_upgrade(
        self: Box<Self>,
        on_upgrade: Box<dyn FnOnce(Box<dyn WebSocketStream>) + Send + Sync + 'static>,
    ) -> BoxFuture<'static, HReply>;
}

pub trait CubTenant: Send + Sync + 'static {
    /// Returns the tenant config
    fn tc(&self) -> &TenantConfig;

    /// Returns the tenant info
    fn ti(&self) -> &Arc<TenantInfo>;

    /// Mark this version as current and broadcast any development client to switch to it.
    fn switch_to(&self, rev: IndexedRevision);

    /// Return the current revision state
    fn revstate(&self) -> CubRevisionState;

    /// Write to the revision state
    fn write_to_revstate(&self, f: &mut dyn FnMut(&mut CubRevisionState));

    fn broadcast_error(&self, e: RevisionError);

    /// Returns the current indexed revision
    fn rev(&self) -> Result<IndexedRevision, conflux::RevisionError> {
        let rs = self.revstate();
        rs.rev.clone().ok_or_else(|| {
            rs.err
                .clone()
                .unwrap_or_else(|| conflux::RevisionError("No revision is loaded".into()))
        })
    }

    /// Returns the revision config
    fn rc(&self) -> eyre::Result<RevisionConfig> {
        let rs = self.revstate();
        rs.rev
            .as_ref()
            .map(|irev| irev.rev.pak.rc.clone())
            .ok_or_else(|| eyre::eyre!("No revision loaded, cannot get RevisionConfig"))
    }

    /// Return the current list of sponsors
    fn sponsors(&self) -> Arc<Sponsors>;

    /// Returns the tenant's object store
    fn store(&self) -> Arc<dyn ObjectStore>;

    /// Return a tenant mom client
    fn tcli(&self) -> Arc<dyn MomTenantClient>;

    /// Returns a future that resolves to the port for the local vite server
    /// (or an error)
    fn vite_port(&self) -> BoxFuture<'_, Result<u16, String>>;

    /// Get search index
    fn index(&self) -> Result<Arc<dyn Index>, conflux::RevisionError>;

    /// Get templates
    fn templates(&self) -> Result<Arc<dyn TemplateCollection>, conflux::RevisionError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PathMetadata {
    pub ctime: OffsetDateTime,
    pub mtime: OffsetDateTime,
    pub len: u64,
    pub is_file: bool,
}

impl PathMetadata {
    pub fn is_file(&self) -> bool {
        self.is_file
    }

    pub fn is_dir(&self) -> bool {
        !self.is_file
    }
}

impl From<std::fs::Metadata> for PathMetadata {
    fn from(metadata: std::fs::Metadata) -> Self {
        PathMetadata {
            ctime: metadata.created().unwrap().into(),
            mtime: metadata.modified().unwrap().into(),
            len: metadata.len(),
            is_file: metadata.is_file(),
        }
    }
}

#[derive(Clone)]
pub struct CubRevisionState {
    // if non-null, we can serve that
    pub rev: Option<IndexedRevision>,

    // if non-null, there was an error loading an initial revision
    // or making a fromscratch/iterative revision
    pub err: Option<RevisionError>,
}

impl CubRevisionState {
    pub fn indexed_rev(&self) -> Result<&IndexedRevision, RevisionError> {
        match (&self.rev, &self.err) {
            (Some(rev), _) => Ok(rev),
            (None, Some(e)) => Err(e.clone()),
            (None, None) => Err(RevisionError(
                "No revision available (and no error)".to_string(),
            )),
        }
    }

    pub fn index(&self) -> Result<&Arc<dyn Index>, RevisionError> {
        Ok(&self.indexed_rev()?.index)
    }

    pub fn templates(&self) -> Result<&Arc<dyn TemplateCollection>, RevisionError> {
        Ok(&self.indexed_rev()?.templates)
    }
}
