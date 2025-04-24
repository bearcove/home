use autotrait::autotrait;
use config_types::{MOM_DEV_API_KEY, MomApiKey, production_mom_url};
use eyre::bail;
use futures_core::future::BoxFuture;
use mom_types::{
    DeriveParams, DeriveResponse, ListMissingArgs, ListMissingResponse, MomEvent, TranscodeParams,
    TranscodeResponse,
    media_types::{HeadersMessage, TranscodeEvent, UploadDoneMessage, WebSocketMessage},
};
use std::str::FromStr;

use libhttpclient::{
    HeaderMap, HeaderValue, Uri,
    header::{self},
};
use merde::IntoStatic;
use std::{sync::Arc, time::Instant};
use tracing::info;

use bytes::Bytes;
use conflux::RevisionIdRef;
use credentials::AuthBundle;
use libgithub::{GitHubCallbackArgs, GitHubCallbackResponse};
use libhttpclient::{HttpClient, RequestBuilder};
use libpatreon::{
    PatreonCallbackArgs, PatreonCallbackResponse, PatreonRefreshCredentials,
    PatreonRefreshCredentialsArgs,
};
use objectstore_types::ObjectStoreKeyRef;

pub trait MomEventListener: Send + 'static {
    fn on_event<'fut>(&'fut self, event: MomEvent) -> BoxFuture<'fut, ()>;
}

pub use eyre::Result;

struct ModImpl;

pub fn load() -> &'static dyn Mod {
    static MOD: ModImpl = ModImpl;
    &MOD
}

#[autotrait]
impl Mod for ModImpl {
    fn client(
        &'static self,
        mcc: MomClientConfig,
    ) -> BoxFuture<'static, Result<Box<dyn MomClient>>> {
        Box::pin(async move {
            let hclient = libhttpclient::load().client();
            let hclient: Arc<dyn HttpClient> = Arc::from(hclient);

            let mclient = MomClientImpl { hclient, mcc };
            let mclient: Box<dyn MomClient> = Box::new(mclient);
            Ok(mclient)
        })
    }

    fn subscribe_to_mom_events(
        &'static self,
        ev_listener: Box<dyn MomEventListener>,
        mcc: MomClientConfig,
    ) -> BoxFuture<'static, Result<()>> {
        Box::pin(async move {
            let (ev_tx, mut ev_rx) = tokio::sync::mpsc::channel(128);

            let relay_fut = {
                async move {
                    let base_uri = Uri::try_from(mcc.base_url.clone()).unwrap();

                    let uri = Uri::builder()
                        .scheme(if base_uri.scheme_str() == Some("https") {
                            "wss"
                        } else {
                            "ws"
                        })
                        .authority(base_uri.authority().unwrap().as_str())
                        .path_and_query("/events")
                        .build()
                        .unwrap();

                    'connect_loop: loop {
                        tracing::debug!(%uri, "Connecting to mom...");
                        async fn random_sleep() {
                            let jitter = rand::random::<u64>() % 500;
                            tokio::time::sleep(std::time::Duration::from_millis(1000 + jitter))
                                .await;
                        }

                        let before = Instant::now();
                        let mod_websock = libwebsock::load();

                        let mut ws = match tokio::time::timeout(
                            std::time::Duration::from_secs(3),
                            mod_websock.websocket_connect(uri.clone(), {
                                let mut map = HeaderMap::new();
                                map.insert(
                                    libhttpclient::header::AUTHORIZATION,
                                    HeaderValue::from_str(&format!("Bearer {}", mcc.api_key()))
                                        .unwrap(),
                                );
                                map
                            }),
                        )
                        .await
                        {
                            Ok(Ok(res)) => res,
                            Ok(Err(e)) => {
                                tracing::warn!("Failed to connect to mom: {}", e);
                                random_sleep().await;
                                continue 'connect_loop;
                            }
                            Err(_) => {
                                tracing::warn!("Timeout connecting to mom");
                                random_sleep().await;
                                continue 'connect_loop;
                            }
                        };
                        let elapsed = before.elapsed();
                        tracing::info!(%uri, ?elapsed, "🧸 mom connection established!");

                        #[allow(unused_labels)]
                        'receive_loop: loop {
                            let before_recv = Instant::now();
                            let ev = match ws.receive().await {
                                None => {
                                    tracing::warn!("Connection closed by mom");
                                    tracing::warn!("...will reconnect now");
                                    continue 'connect_loop;
                                }
                                Some(Ok(ev)) => ev,
                                Some(Err(e)) => {
                                    tracing::warn!("Failed to receive mom event: {e}");
                                    tracing::warn!("...will reconnect now");
                                    continue 'connect_loop;
                                }
                            };

                            let ev = match ev {
                                libwebsock::Frame::Text(ev) => ev,
                                _ => {
                                    bail!("Expected text frame")
                                }
                            };

                            let ev = merde::json::from_str_owned::<MomEvent>(&ev)
                                .map_err(|e| e.into_static())?;
                            let elapsed = before_recv.elapsed();
                            tracing::debug!(?ev, ?elapsed, "Got event from mom");

                            _ = ev_tx.send(ev).await;
                        }
                    }
                }
            };

            tokio::spawn(async move {
                let res: Result<()> = relay_fut.await;
                if let Err(e) = res {
                    tracing::error!("Failed to relay mom events: {e}");
                    tracing::error!(
                        "Is the local mom newer? Maybe? If the schema changed, you can develop locally by exporting the environment variable FORCE_LOCAL_MOM=1"
                    );
                }
            });

            tokio::spawn({
                async move {
                    while let Some(ev) = ev_rx.recv().await {
                        ev_listener.on_event(ev).await;
                    }
                }
            });

            Ok(())
        })
    }
}

/// Configuration for a Mom client.
///
/// Contains the base URL of the Mom server and the API key required for authentication.
#[derive(Clone)]
pub struct MomClientConfig {
    /// The base URL of the Mom server.
    pub base_url: String,
    /// The API key used to authenticate with the Mom server.
    pub api_key: Option<MomApiKey>,
}

impl MomClientConfig {
    /// Creates a new `MomClientConfig` with the given base URL and API key.
    pub fn api_key(&self) -> MomApiKey {
        self.api_key.clone().unwrap_or_else(|| {
            eprintln!("==================================================");
            eprintln!("=                                                =");
            eprintln!("=      WARNING: set $MOM_API_KEY to something    =");
            eprintln!("=      real to deploy                            =");
            eprintln!("=                                                =");
            eprintln!("==================================================");
            MOM_DEV_API_KEY.to_owned()
        })
    }
}

struct MomClientImpl {
    hclient: Arc<dyn HttpClient>,
    mcc: MomClientConfig,
}

#[autotrait]
impl MomClient for MomClientImpl {
    fn mom_tenant_client(
        &self,
        tenant_name: config_types::TenantDomain,
    ) -> Box<dyn MomTenantClient> {
        Box::new(MomTenantClientImpl {
            base_path: format!("/tenant/{tenant_name}"),
            hclient: self.hclient.clone(),
            mcc: self.mcc.clone(),
        })
    }
}

struct MomTenantClientImpl {
    mcc: MomClientConfig,
    base_path: String,
    hclient: Arc<dyn HttpClient>,
}

impl MomTenantClientImpl {
    /// Makes a URL for the mom server, for login/auth purposes
    /// note: path is a relative path, like `objectstore/list-missing` (no leading slash)
    fn config_mom_uri(&self, relative_path: &str) -> Uri {
        let base_url = Uri::from_str(&self.mcc.base_url).unwrap();
        let full_path = format!("{}/{}", self.base_path, relative_path);
        Uri::builder()
            .scheme(base_url.scheme_str().unwrap_or("https"))
            .authority(base_url.authority().unwrap().as_str())
            .path_and_query(&full_path)
            .build()
            .unwrap()
    }

    /// Makes a URL for the mom server, for revision/asset uploads
    /// note: path is a relative path, like `objectstore/list-missing` (no leading slash)
    fn prod_mom_url(&self, relative_path: &str) -> (String, Uri) {
        use config_types::is_development;

        let base_url = if is_development() {
            production_mom_url().to_string()
        } else {
            self.mcc.base_url.clone()
        };
        let full_path = format!("{}/{}", self.base_path, relative_path);
        let url = format!("{base_url}{full_path}");
        let uri = Uri::from_str(&url).unwrap();
        (url, uri)
    }
}

#[autotrait]
impl MomTenantClient for MomTenantClientImpl {
    fn update_auth_bundle<'fut>(
        &'fut self,
        body: &'fut AuthBundle,
    ) -> BoxFuture<'fut, Result<AuthBundle>> {
        Box::pin({
            async move {
                let uri = self.config_mom_uri("auth-bundle/update");
                let req = self.hclient.post(uri).with_auth(&self.mcc).json(body)?;
                let res = req.send_and_expect_200().await?;
                Ok(res.json::<AuthBundle>().await?)
            }
        })
    }

    fn github_callback<'fut>(
        &'fut self,
        body: &'fut GitHubCallbackArgs,
    ) -> BoxFuture<'fut, Result<Option<GitHubCallbackResponse>>> {
        Box::pin({
            async move {
                let uri = self.config_mom_uri("github/callback");
                let req = self.hclient.post(uri).with_auth(&self.mcc).json(body)?;
                let res = req.send_and_expect_200().await?;
                Ok(res.json::<Option<GitHubCallbackResponse>>().await?)
            }
        })
    }

    fn patreon_callback<'fut>(
        &'fut self,
        body: &'fut PatreonCallbackArgs,
    ) -> BoxFuture<'fut, Result<Option<PatreonCallbackResponse>>> {
        Box::pin({
            async move {
                let uri = self.config_mom_uri("patreon/callback");
                let req = self.hclient.post(uri).with_auth(&self.mcc).json(body)?;
                let res = req.send_and_expect_200().await?;
                Ok(res.json::<Option<PatreonCallbackResponse>>().await?)
            }
        })
    }

    fn patreon_refresh_credentials<'fut>(
        &'fut self,
        body: &'fut PatreonRefreshCredentialsArgs,
    ) -> BoxFuture<'fut, Result<PatreonRefreshCredentials>> {
        Box::pin({
            async move {
                let uri = self.config_mom_uri("patreon/refresh-credentials");
                let req = self.hclient.post(uri).with_auth(&self.mcc).json(body)?;
                let res = req.send_and_expect_200().await?;
                Ok(res.json::<PatreonRefreshCredentials>().await?)
            }
        })
    }

    fn objectstore_list_missing<'fut>(
        &'fut self,
        body: &'fut ListMissingArgs,
    ) -> BoxFuture<'fut, Result<ListMissingResponse>> {
        Box::pin({
            async move {
                let (_, uri) = self.prod_mom_url("objectstore/list-missing");
                let req = self.hclient.post(uri).with_auth(&self.mcc).json(body)?;
                let res = req.send_and_expect_200().await?;
                Ok(res.json::<ListMissingResponse>().await?)
            }
        })
    }

    fn put_asset<'fut>(
        &'fut self,
        key: &'fut ObjectStoreKeyRef,
        payload: Bytes,
    ) -> BoxFuture<'fut, Result<()>> {
        Box::pin({
            async move {
                let (_, uri) = self.prod_mom_url(&format!("objectstore/put/{key}"));
                self.hclient
                    .put(uri)
                    .with_auth(&self.mcc)
                    .body(payload)
                    .send_and_expect_200()
                    .await?;
                Ok(())
            }
        })
    }

    fn put_revpak<'fut>(
        &'fut self,
        id: &'fut RevisionIdRef,
        payload: Bytes,
    ) -> BoxFuture<'fut, Result<()>> {
        Box::pin({
            let revision_id: &RevisionIdRef = id;
            async move {
                let (_, uri) = self.prod_mom_url(&format!("revision/upload/{revision_id}"));
                info!("Uploading revision to URL: {}", uri);
                self.hclient
                    .put(uri)
                    .with_auth(&self.mcc)
                    .body(payload)
                    .send_and_expect_200()
                    .await?;
                Ok(())
            }
        })
    }

    fn media_transcode(&self, params: TranscodeParams) -> BoxFuture<'_, Result<TranscodeResponse>> {
        Box::pin(async move {
            let uri = self.config_mom_uri("media/transcode");
            let req = self.hclient.post(uri).with_auth(&self.mcc).json(&params)?;
            let res = req.send().await?;
            let response: TranscodeResponse = res.json().await?;
            Ok(response)
        })
    }

    fn derive(&self, params: DeriveParams) -> BoxFuture<'_, Result<DeriveResponse>> {
        Box::pin(async move {
            let uri = self.config_mom_uri("derive");
            let req = self.hclient.post(uri).with_auth(&self.mcc).json(&params)?;
            let res = req.send().await?;
            let response: DeriveResponse = res.json().await?;
            Ok(response)
        })
    }

    fn media_uploader(
        &self,
        listener: Box<dyn TranscodingEventListener>,
    ) -> BoxFuture<'_, Result<Box<dyn MediaUploader>>> {
        Box::pin(async move {
            let base_uri = self.config_mom_uri("media/upload");
            let uri = Uri::builder()
                .scheme(if base_uri.scheme_str() == Some("https") {
                    "wss"
                } else {
                    "ws"
                })
                .authority(base_uri.authority().unwrap().as_str())
                .path_and_query(base_uri.path_and_query().unwrap().as_str())
                .build()
                .unwrap();
            info!("Uploading video to: {uri}");

            let ws = libwebsock::load()
                .websocket_connect(uri, {
                    let mut map = HeaderMap::new();
                    map.insert(
                        libhttpclient::header::AUTHORIZATION,
                        HeaderValue::from_str(&format!("Bearer {}", self.mcc.api_key())).unwrap(),
                    );
                    map
                })
                .await?;

            let b: Box<dyn MediaUploader> = Box::new(MediaUploaderImpl { ws, listener });
            Ok(b)
        })
    }
}

struct MediaUploaderImpl {
    ws: Box<dyn libwebsock::WebSocketStream>,
    listener: Box<dyn TranscodingEventListener>,
}

#[autotrait(!Sync)]
impl MediaUploader for MediaUploaderImpl {
    fn with_headers(&mut self, headers: HeadersMessage) -> BoxFuture<'_, Result<()>> {
        Box::pin(async move {
            let msg = WebSocketMessage::Headers(headers);
            let json = merde::json::to_string(&msg)?;
            self.ws.send_text(json).await?;
            Ok(())
        })
    }

    fn upload_chunk(&mut self, chunk: Bytes) -> BoxFuture<'_, Result<()>> {
        Box::pin(async move {
            self.ws.send_binary(chunk.to_vec()).await?;
            Ok(())
        })
    }

    fn done_and_download_result<'a>(
        &'a mut self,
        uploaded_size: usize,
        mut chunk_receiver: Box<dyn ChunkReceiver + 'a>,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            tracing::debug!("Sending UploadDone message with size {uploaded_size}");
            let msg = WebSocketMessage::UploadDone(UploadDoneMessage { uploaded_size });
            let json = merde::json::to_string(&msg)?;
            self.ws.send_text(json).await?;

            let mut received_bytes = 0;

            loop {
                tracing::trace!("Waiting for next websocket message...");
                let msg = match self.ws.receive().await {
                    Some(frame) => frame,
                    None => {
                        bail!("Connection closed unexpectedly (but gracefully)");
                    }
                }?;
                match msg {
                    libwebsock::Frame::Text(text) => {
                        let msg: WebSocketMessage =
                            merde::json::from_str(&text).map_err(|e| e.into_static())?;
                        match msg {
                            WebSocketMessage::TranscodingEvent(ev) => {
                                if let Err(e) = self.listener.on_transcoding_event(ev).await {
                                    bail!("Could not notify progress: {e}");
                                }
                            }
                            WebSocketMessage::TranscodingComplete(complete) => {
                                let size = complete.output_size;
                                tracing::info!(
                                    "Transcoding complete! Expecting {size} bytes of output"
                                );

                                // Start receiving binary frames and forwarding them
                                loop {
                                    let res = match self.ws.receive().await {
                                        Some(res) => res,
                                        None => {
                                            tracing::error!(
                                                "WebSocket connection closed unexpectedly"
                                            );
                                            bail!("WebSocket connection closed unexpectedly");
                                        }
                                    };
                                    match res? {
                                        libwebsock::Frame::Binary(chunk) => {
                                            received_bytes += chunk.len();
                                            tracing::trace!(
                                                "Received chunk of {} bytes ({}/{} total)",
                                                chunk.len(),
                                                received_bytes,
                                                size
                                            );
                                            // Forward chunk using chunk receiver
                                            chunk_receiver.on_chunk(chunk).await?;

                                            if received_bytes == size {
                                                tracing::info!(
                                                    "Successfully received complete response ({size} bytes)"
                                                );
                                                return Ok(());
                                            }
                                        }
                                        _ => {
                                            bail!("Expected binary frame");
                                        }
                                    }
                                }
                            }
                            WebSocketMessage::Error(err) => {
                                tracing::error!("Received error from transcoding server: {err}");
                                bail!("{err}");
                            }
                            _ => {
                                bail!("Unexpected message type");
                            }
                        }
                    }
                    _ => {
                        bail!("Expected text message");
                    }
                }
            }
        })
    }
}

pub trait TranscodingEventListener: Send + Sync + 'static {
    fn on_transcoding_event(&self, ev: TranscodeEvent) -> BoxFuture<'_, Result<()>>;
}

pub trait ChunkReceiver: Send + Sync {
    fn on_chunk(&mut self, chunk: Vec<u8>) -> BoxFuture<'_, Result<()>>;
}

trait WithAuth {
    fn with_auth(self: Box<Self>, mcc: &MomClientConfig) -> Box<dyn RequestBuilder>;
}

impl WithAuth for dyn RequestBuilder {
    fn with_auth(self: Box<Self>, mcc: &MomClientConfig) -> Box<dyn RequestBuilder> {
        use libhttpclient::header::HeaderValue;
        self.header(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", mcc.api_key())).unwrap(),
        )
    }
}
