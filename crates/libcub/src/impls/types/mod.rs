use config_types::{
    CubConfig, TenantConfig, TenantDomain, TenantInfo, WebConfig, is_development, is_production,
};
use conflux::{RevisionError, RevisionId};
use cub_types::{CubRevisionState, CubTenant, IndexedRevision};
use hattip::prelude::BoxFuture;
use libmomclient::{MomClient, MomTenantClient};
use libobjectstore::ObjectStore;
use mom_types::{GlobalStateView, Sponsors};
use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc};
use template_types::TemplateCollection;
use tokio::sync::broadcast;
use tower_cookies::Key;

use super::{global_state, vite::start_vite};

#[derive(Clone)]
pub enum RevisionBroadcastEvent {
    NewRevision(RevisionId),
    RevisionError(String),
}

merde::derive!(
    impl (Serialize, Deserialize) for
    enum RevisionBroadcastEvent externally_tagged {
        "new_revision" => NewRevision,
        "revision_error" => RevisionError,
    }
);

#[derive(Clone)]
pub enum DomainResolution {
    /// This domain maps directly to a tenant
    Tenant(Arc<CubTenantImpl>),
    /// This domain should redirect to another domain
    Redirect {
        /// The target domain to redirect to
        target_domain: TenantDomain,
        /// The tenant this domain is associated with
        tenant: Arc<CubTenantImpl>,
    },
}

pub struct CubGlobalState {
    /// config
    pub config: CubConfig,

    /// web config
    pub web: WebConfig,

    /// shared mom client
    pub mom_client: Arc<dyn MomClient>,

    /// shared mom deploy client
    pub mom_deploy_client: Arc<dyn MomClient>,

    /// this state can be updated by mom's messages (adding/removing tenants etc.)
    pub dynamic: Arc<RwLock<CubDynamicState>>,
}

pub struct CubDynamicState {
    /// Map of tenants by their name (e.g., "fasterthanli.me")
    pub tenants_by_name: HashMap<TenantDomain, Arc<CubTenantImpl>>,

    /// Domain resolution map that includes both direct tenant mappings and redirect mappings
    /// Maps domains (e.g., "fasterthanli.me") to their resolution
    pub domain_resolution: HashMap<TenantDomain, DomainResolution>,
}

impl CubGlobalState {
    /// Build a redirect URL from the original request to the target domain
    pub fn build_redirect_url(
        &self,
        target_domain: &TenantDomain,
        original_uri: &axum::http::Uri,
        original_host: &str,
    ) -> url::Url {
        // Determine the scheme based on environment
        let scheme = if is_production() { "https" } else { "http" };

        // Create a base URL from the original request
        let url_str = format!("{scheme}://{original_host}{original_uri}");
        let mut url = url::Url::parse(&url_str).expect("Failed to parse original URL");

        // Replace the host with the target domain
        let port = if is_development() {
            Some(self.web.port)
        } else {
            None
        };

        url.set_host(Some(target_domain.as_str()))
            .expect("Failed to set host");
        if let Some(port) = port {
            url.set_port(Some(port)).expect("Failed to set port");
        }

        url
    }
}

pub struct CubTenantImpl {
    // FIXME: should not be static — that means we leak it
    pub cookie_key: &'static Key,
    pub sponsors: RwLock<Arc<Sponsors>>,
    pub ti: Arc<TenantInfo>,
    pub store: Arc<dyn ObjectStore>,
    pub bx_rev: broadcast::Sender<RevisionBroadcastEvent>,
    pub rev_state: RwLock<CubRevisionState>,
    pub vite_port: tokio::sync::OnceCell<Result<u16, String>>,
}

impl CubTenant for CubTenantImpl {
    fn tc(&self) -> &TenantConfig {
        &self.ti.tc
    }

    fn ti(&self) -> &Arc<TenantInfo> {
        &self.ti
    }

    fn switch_to(&self, rev: IndexedRevision) {
        let rev_id = rev.rev.id().clone();
        let tenant_name = self.tc().name.clone();

        {
            let pak = rev.rev.pak.clone();
            let info = self.ti.clone();
            tokio::task::spawn(async move {
                let before_save = std::time::Instant::now();
                librevision::load()
                    .save_pak_to_disk_as_active(&pak, &info)
                    .await
                    .unwrap();
                tracing::info!(
                    "[{tenant_name}] Saving pak to disk took {:?}",
                    before_save.elapsed()
                );
            });
        }

        let mut rs = self.rev_state.write();
        *rs = CubRevisionState {
            rev: Some(rev.clone()),
            err: None,
        };

        match self
            .bx_rev
            .send(RevisionBroadcastEvent::NewRevision(rev_id.clone()))
        {
            Ok(n) => tracing::info!("Notified {n} clients about {rev_id}"),
            Err(e) => tracing::error!("Failed to broadcast revision: {e}"),
        }
    }

    fn revstate(&self) -> CubRevisionState {
        self.rev_state.read().clone()
    }

    /// Write to the revision state
    fn write_to_revstate(&self, f: &mut dyn FnMut(&mut CubRevisionState)) {
        let mut state = self.rev_state.write();
        f(&mut state);
    }

    /// Broadcast an error
    fn broadcast_error(&self, e: RevisionError) {
        let err_for_clients = conflux::RevisionError(
            libterm::load().format_ansi(&e.0, libterm::FormatAnsiStyle::Html),
        );

        match self
            .bx_rev
            .send(RevisionBroadcastEvent::RevisionError(err_for_clients.0))
        {
            Ok(n) => {
                tracing::warn!("Notified {n} clients about an error building a revision: {e}")
            }
            Err(e) => tracing::error!("Failed to broadcast revision: {e}"),
        }
    }

    fn sponsors(&self) -> Arc<Sponsors> {
        self.sponsors.read().clone()
    }

    fn store(&self) -> Arc<dyn ObjectStore> {
        self.store.clone()
    }

    fn tcli(&self) -> Arc<dyn MomTenantClient> {
        Arc::from(
            global_state()
                .mom_client
                .mom_tenant_client(self.ti.tc.name.clone()),
        )
    }

    fn vite_port(&self) -> BoxFuture<'_, Result<u16, String>> {
        Box::pin(async move {
            self.vite_port
                .get_or_init(|| {
                    let ti = self.ti.clone();
                    Box::pin(async move {
                        start_vite(ti, global_state().web)
                            .await
                            .map_err(|e| format!("{e:?}"))
                    })
                })
                .await
                .clone()
        })
    }

    /// Get search index
    fn index(&self) -> Result<Arc<dyn libsearch::Index>, conflux::RevisionError> {
        self.revstate().index().map(Arc::clone)
    }

    /// Get templates
    fn templates(&self) -> Result<Arc<dyn TemplateCollection>, conflux::RevisionError> {
        self.revstate().templates().map(Arc::clone)
    }
}

impl GlobalStateView for CubTenantImpl {
    fn gsv_ti(&self) -> Arc<TenantInfo> {
        CubTenant::ti(self).clone()
    }

    fn gsv_sponsors(&self) -> Arc<Sponsors> {
        CubTenant::sponsors(self)
    }
}

impl CubTenantImpl {
    /// Get private cookies
    pub fn private_cookies(
        &self,
        cookies: tower_cookies::Cookies,
    ) -> tower_cookies::PrivateCookies<'static> {
        cookies.private(self.cookie_key)
    }
}
