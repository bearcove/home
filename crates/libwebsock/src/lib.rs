use autotrait::autotrait;
use eyre::eyre;
use libhttpclient::Bytes;
use std::net::IpAddr;
pub use tokio_tungstenite::tungstenite::{Message, protocol::frame::CloseFrame};

use http::{HeaderMap, Uri};
use rubicon as _;

use rustls as _;

use futures_core::future::BoxFuture;

struct ModImpl;

pub fn load() -> &'static dyn Mod {
    static MOD: ModImpl = ModImpl;
    &MOD
}

#[autotrait]
impl Mod for ModImpl {
    fn websocket_connect(
        &self,
        uri: Uri,
        headers: HeaderMap,
    ) -> BoxFuture<'_, eyre::Result<Box<dyn WebSocketStream>>> {
        Box::pin(async move {
            use std::time::Instant;

            let mut request = uri.clone().into_client_request()?;
            request.headers_mut().extend(headers);

            let host = uri.host().ok_or_else(|| eyre!("Missing host"))?;
            let scheme = uri.scheme_str().ok_or_else(|| eyre!("Missing scheme"))?;
            let port = uri
                .port_u16()
                .unwrap_or(if scheme == "wss" || scheme == "https" {
                    443
                } else {
                    80
                });
            let host_and_port = format!("{host}:{port}");
            log::debug!("Resolving {host_and_port}");

            let before_dns = Instant::now();
            let ip: IpAddr = if let Ok(ipv4) = host.parse::<std::net::Ipv4Addr>() {
                ipv4.into()
            } else if let Ok(_ipv6) = host.parse::<std::net::Ipv6Addr>() {
                // If a literal IPv6 address was given, skip it (since we only want IPv4)
                return Err(eyre!("IPv6 addresses not supported, only IPv4"));
            } else {
                let mut addrs = tokio::net::lookup_host((host, port))
                    .await
                    .map_err(|e| eyre!("Failed to resolve host: {e}"))?
                    .filter_map(|sa| match sa {
                        std::net::SocketAddr::V4(addr) => Some(IpAddr::V4(*addr.ip())),
                        std::net::SocketAddr::V6(addr) => {
                            // If it's ::1 (IPv6 localhost), return 127.0.0.1 (IPv4 localhost) instead.
                            if addr.ip() == &std::net::Ipv6Addr::LOCALHOST {
                                Some(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST))
                            } else {
                                None
                            }
                        }
                    });

                addrs
                    .next()
                    .ok_or_else(|| eyre!("Failed to resolve host (no IPv4 addresses found)"))?
            };
            let dns_elapsed = before_dns.elapsed();

            log::debug!("Resolved {host_and_port} to {ip} in {dns_elapsed:?}");

            log::debug!("Connecting to {ip}:{port}...");
            let before_tcp = Instant::now();
            let stream = tokio::net::TcpStream::connect((ip, port))
                .await
                .map_err(|e| eyre!("Failed to establish TCP connection: {e}"))?;
            let tcp_elapsed = before_tcp.elapsed();

            stream
                .set_nodelay(true)
                .map_err(|e| eyre!("Failed to set TCP_NODELAY: {e}"))?;

            log::debug!("TCP connection established in {tcp_elapsed:?}");
            log::debug!("Doing websocket handshake...");

            let before_handshake = Instant::now();
            let (ws_stream, _) = tokio_tungstenite::client_async_tls_with_config(
                request,
                stream,
                Some(WebSocketConfig::default()),
                None,
            )
            .await
            .map_err(|e| {
                log::warn!("WebSocket handshake failed: {e}");
                eyre!("Failed to complete WebSocket handshake: {e}")
            })?;
            let handshake_elapsed = before_handshake.elapsed();

            log::debug!("WebSocket handshake completed in {handshake_elapsed:?}");

            Ok(Box::new(WebSocketStreamImpl::new(ws_stream)) as Box<dyn WebSocketStream>)
        })
    }
}

use tokio_tungstenite::{
    MaybeTlsStream,
    tungstenite::{client::IntoClientRequest, protocol::WebSocketConfig},
};

type Wss = tokio_tungstenite::WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

struct WebSocketStreamImpl {
    inner: Wss,
}

impl WebSocketStreamImpl {
    fn new(inner: Wss) -> Self {
        Self { inner }
    }
}

#[autotrait(!Sync)]
impl WebSocketStream for WebSocketStreamImpl {
    fn send(&mut self, frame: Message) -> BoxFuture<'_, eyre::Result<()>> {
        use futures_util::SinkExt;
        Box::pin(async move {
            self.inner
                .send(frame)
                .await
                .map_err(|e| eyre!("Failed to send WebSocket message: {}", e))?;
            Ok(())
        })
    }

    fn send_binary(&mut self, msg: Bytes) -> BoxFuture<'_, eyre::Result<()>> {
        Box::pin(async move { self.send(Message::Binary(msg)).await })
    }

    fn send_text(&mut self, msg: String) -> BoxFuture<'_, eyre::Result<()>> {
        Box::pin(async move { self.send(Message::Text(msg.into())).await })
    }

    fn receive(&mut self) -> BoxFuture<'_, Option<eyre::Result<Message>>> {
        use futures_util::StreamExt;
        Box::pin(async move {
            let res = match self.inner.next().await? {
                Ok(msg) => Ok(msg),
                Err(e) => Err(eyre!("Failed to receive WebSocket message: {}", e)),
            };
            Some(res)
        })
    }
}
