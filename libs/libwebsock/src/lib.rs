include!(".dylo/spec.rs");
include!(".dylo/support.rs");

use std::net::IpAddr;

use http::{HeaderMap, Uri};
use rubicon as _;
use tokio_proxy as _;

#[cfg(feature = "impl")]
use rustls as _;

use futures_core::future::BoxFuture;

pub use libhttpclient::Error;

#[cfg(feature = "impl")]
#[derive(Default)]
struct ModImpl;

#[derive(Debug)]
pub struct CloseFrame {
    pub code: u16,
    pub reason: String,
}

#[derive(Debug)]
pub enum Frame {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close(Option<CloseFrame>),
}

#[dylo::export]
impl Mod for ModImpl {
    fn websocket_connect(
        &self,
        uri: Uri,
        headers: HeaderMap,
    ) -> BoxFuture<'_, Result<Box<dyn WebSocketStream>, Error>> {
        Box::pin(async move {
            use std::time::Instant;

            let mut request = uri
                .clone()
                .into_client_request()
                .map_err(|e| Error::Any(e.to_string()))?;
            request.headers_mut().extend(headers);

            let host = uri
                .host()
                .ok_or_else(|| Error::Any("Missing host".to_string()))?;
            let scheme = uri
                .scheme_str()
                .ok_or_else(|| Error::Any("Missing scheme".to_string()))?;
            let port = uri
                .port_u16()
                .unwrap_or(if scheme == "wss" || scheme == "https" {
                    443
                } else {
                    80
                });
            let host_and_port = format!("{host}:{port}");
            tracing::debug!("Resolving {host_and_port}");

            let before_dns = Instant::now();
            let ip: IpAddr = if let Ok(ipv4) = host.parse::<std::net::Ipv4Addr>() {
                ipv4.into()
            } else if let Ok(ipv6) = host.parse::<std::net::Ipv6Addr>() {
                ipv6.into()
            } else {
                let resolv_conf = tokio::fs::read_to_string("/etc/resolv.conf")
                    .await
                    .map_err(|e| Error::Any(format!("Failed to read /etc/resolv.conf: {e}")))?;
                let nameserver = resolv_conf
                    .lines()
                    .find(|line| line.starts_with("nameserver"))
                    .and_then(|line| line.split_whitespace().nth(1))
                    .and_then(|ip| ip.parse::<std::net::Ipv4Addr>().ok())
                    .ok_or_else(|| {
                        Error::Any("No valid nameserver found in /etc/resolv.conf".to_string())
                    })?;
                tracing::debug!("Using nameserver {nameserver}");

                let config = hickory_resolver::config::ResolverConfig::from_parts(
                    None,
                    vec![],
                    vec![hickory_resolver::config::NameServerConfig {
                        socket_addr: (nameserver, 53).into(),
                        protocol: hickory_resolver::config::Protocol::Udp,
                        tls_dns_name: None,
                        trust_negative_responses: false,
                        bind_addr: None,
                    }],
                );

                let resolver = hickory_resolver::TokioAsyncResolver::tokio(
                    config,
                    hickory_resolver::config::ResolverOpts::default(),
                );
                let ipv4_lookup = resolver
                    .ipv4_lookup(host)
                    .await
                    .map_err(|e| Error::Any(e.to_string()))?;
                ipv4_lookup
                    .iter()
                    .next()
                    .ok_or_else(|| Error::Any("Failed to resolve host".to_string()))?
                    .0
                    .into()
            };
            let dns_elapsed = before_dns.elapsed();

            // TODO: don't trouble google dns for localhost...
            tracing::debug!("Resolved {host_and_port} to {ip} in {dns_elapsed:?}");

            tracing::debug!("Connecting to {ip}:{port}...");
            let before_tcp = Instant::now();
            let stream = tokio::net::TcpStream::connect((ip, port))
                .await
                .map_err(|e| Error::Any(format!("Failed to establish TCP connection: {e}")))?;
            let tcp_elapsed = before_tcp.elapsed();

            stream
                .set_nodelay(true)
                .map_err(|e| Error::Any(format!("Failed to set TCP_NODELAY: {e}")))?;

            tracing::debug!("TCP connection established in {tcp_elapsed:?}");
            tracing::debug!("Doing websocket handshake...");

            let before_handshake = Instant::now();
            let (ws_stream, _) = tokio_tungstenite::client_async_tls_with_config(
                request,
                stream,
                Some(WebSocketConfig::default()),
                None,
            )
            .await
            .map_err(|e| {
                tracing::warn!("WebSocket handshake failed: {e}");
                Error::Any(format!("Failed to complete WebSocket handshake: {e}"))
            })?;
            let handshake_elapsed = before_handshake.elapsed();

            tracing::debug!("WebSocket handshake completed in {handshake_elapsed:?}");

            Ok(Box::new(WebSocketStreamImpl::new(ws_stream)) as Box<dyn WebSocketStream>)
        })
    }
}

#[cfg(feature = "impl")]
use tokio_tungstenite::{
    MaybeTlsStream,
    tungstenite::{client::IntoClientRequest, protocol::WebSocketConfig},
};

#[cfg(feature = "impl")]
type Wss = tokio_tungstenite::WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

#[cfg(feature = "impl")]
struct WebSocketStreamImpl {
    inner: Wss,
}

#[cfg(feature = "impl")]
impl WebSocketStreamImpl {
    fn new(inner: Wss) -> Self {
        Self { inner }
    }
}

#[dylo::export(nonsync)]
impl WebSocketStream for WebSocketStreamImpl {
    fn send(&mut self, frame: Frame) -> BoxFuture<'_, Result<(), Error>> {
        use futures_util::SinkExt;
        use tokio_tungstenite::tungstenite as tung;
        Box::pin(async move {
            let msg = match frame {
                Frame::Text(text) => tung::Message::Text(text),
                Frame::Binary(data) => tung::Message::Binary(data),
                Frame::Ping(data) => tung::Message::Ping(data),
                Frame::Pong(data) => tung::Message::Pong(data),
                Frame::Close(frame) => {
                    tung::Message::Close(frame.map(|f| tung::protocol::CloseFrame {
                        code: tung::protocol::frame::coding::CloseCode::from(f.code),
                        reason: f.reason.into(),
                    }))
                }
            };
            self.inner
                .send(msg)
                .await
                .map_err(|e| Error::Any(e.to_string()))?;
            Ok(())
        })
    }

    fn send_binary(&mut self, msg: Vec<u8>) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(async move { self.send(Frame::Binary(msg)).await })
    }

    fn send_text(&mut self, msg: String) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(async move { self.send(Frame::Text(msg)).await })
    }

    fn receive(&mut self) -> BoxFuture<'_, Option<Result<Frame, Error>>> {
        use futures_util::StreamExt;
        use tokio_tungstenite::tungstenite as tung;
        Box::pin(async move {
            let res = match self.inner.next().await? {
                Ok(msg) => Ok(match msg {
                    tung::Message::Binary(data) => Frame::Binary(data),
                    tung::Message::Text(text) => Frame::Text(text),
                    tung::Message::Close(close) => Frame::Close(close.map(|cf| CloseFrame {
                        code: cf.code.into(),
                        reason: cf.reason.into_owned(),
                    })),
                    tung::Message::Ping(data) => Frame::Ping(data),
                    tung::Message::Pong(data) => Frame::Pong(data),
                    tung::Message::Frame(_) => {
                        unreachable!("amos doesn't love tungstenite's design")
                    }
                }),
                Err(e) => Err(Error::Any(e.to_string())),
            };
            Some(res)
        })
    }
}
