use std::sync::Arc;

use axum::extract::ws;
use conflux::{AbsoluteUrl, Href, LoadedPage, Route};
use cub_types::CubTenant;
use futures_util::SinkExt;
use http::Uri;
use libhttpclient::{HttpClient, StatusCode};
use url::Url;

use crate::impls::{CubTenantImpl, cub_req::CubReqImpl, global_state};

use super::deploy::{Level, LogMessage};

#[derive(Debug)]
enum ValidationMessage {
    LogMessage(LogMessage),
    RouteResult(RouteResult),
    BadLink(BadLink),
    MathError(MathError),
    ValidationComplete(ValidationComplete),
}

merde::derive! {
    impl (Serialize, Deserialize) for enum ValidationMessage
    externally_tagged {
        "logMessage" => LogMessage,
        "routeResult" => RouteResult,
        "validationComplete" => ValidationComplete,
        "badLink" => BadLink,
        "mathError" => MathError,
    }
}

macro_rules! impl_from {
    ($from:tt) => {
        impl From<$from> for ValidationMessage {
            fn from(value: $from) -> Self {
                ValidationMessage::$from(value)
            }
        }
    };
}
impl_from!(LogMessage);
impl_from!(RouteResult);
impl_from!(ValidationComplete);
impl_from!(BadLink);
impl_from!(MathError);

#[derive(Debug)]
struct RouteResult {
    url: AbsoluteUrl,
    status: u16,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct RouteResult {
        url, status
    }
}

#[derive(Debug)]
struct ValidationComplete {
    num_errors: u32,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct ValidationComplete {
        num_errors
    }
}

#[derive(Debug)]
struct BadLink {
    route: Route,
    href: Href,
    reason: String,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct BadLink {
        route,
        href,
        reason
    }
}

#[derive(Debug)]
struct MathError {
    route: Route,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct MathError {
        route
    }
}

pub(crate) async fn serve(
    ws: axum::extract::WebSocketUpgrade,
    tr: CubReqImpl,
) -> impl axum::response::IntoResponse {
    let ts = tr.tenant.clone();
    ws.on_upgrade(move |ws| handle_validation(ws, ts))
}

struct MsgSender<'a> {
    sock: &'a mut ws::WebSocket,
}

#[allow(dead_code)]
impl MsgSender<'_> {
    async fn send(&mut self, msg: impl Into<ValidationMessage>) {
        let msg: ValidationMessage = msg.into();

        if let Err(e) = self
            .sock
            .send(ws::Message::Text(merde::json::to_string(&msg).unwrap()))
            .await
        {
            tracing::error!("Failed to send WebSocket message: {}", e);
        }
    }

    async fn info<D: std::fmt::Display>(&mut self, message: D) {
        self.send(LogMessage {
            level: Level::Info,
            message: message.to_string(),
        })
        .await;
    }

    async fn warn<D: std::fmt::Display>(&mut self, message: D) {
        self.send(LogMessage {
            level: Level::Warn,
            message: message.to_string(),
        })
        .await;
    }

    async fn error<D: std::fmt::Display>(&mut self, message: D) {
        self.send(LogMessage {
            level: Level::Error,
            message: message.to_string(),
        })
        .await;
    }
}

async fn handle_validation(mut sock: ws::WebSocket, ts: Arc<CubTenantImpl>) {
    let mut ms = MsgSender { sock: &mut sock };

    let irev = match ts.rev() {
        Ok(rev) => rev,
        Err(e) => {
            ms.error(format!("Failed to get current revision: {}", e))
                .await;
            return;
        }
    };

    ms.info("Validating routes...").await;

    enum Task {
        CheckRoute { route: Route },
        CheckPageLinks { page: Arc<LoadedPage> },
    }
    enum Result {
        RouteChecked {
            route: Route,
            status: StatusCode,
        },
        BadLink {
            route: Route,
            href: Href,
            reason: String,
        },
        MathError {
            route: Route,
        },
    }

    let (task_tx, task_rx) = flume::unbounded::<Task>();
    let (res_tx, res_rx) = flume::unbounded::<Result>();

    let client = libhttpclient::load().client();
    let client: Arc<dyn HttpClient> = Arc::from(client);

    // Start four workers
    for _ in 0..4 {
        let client = client.clone();
        let task_rx = task_rx.clone();
        let res_tx = res_tx.clone();
        let ts = ts.clone();

        tokio::spawn(async move {
            while let Ok(task) = task_rx.recv_async().await {
                let base_url = &ts.tc().web_base_url(global_state().web);
                match task {
                    Task::CheckRoute { route } => {
                        let url = format!("{base_url}{route}");
                        tracing::debug!("Working checking {url}");

                        let response = client.get(Uri::try_from(&url).unwrap()).send().await;
                        let status = match response {
                            Ok(res) => res.status(),
                            Err(_) => StatusCode::from_u16(255).unwrap(),
                        };
                        tracing::debug!("Status {status} for {url}");
                        res_tx
                            .send_async(Result::RouteChecked { route, status })
                            .await
                            .unwrap();
                    }
                    Task::CheckPageLinks { page } => {
                        if page.html.contains("<merror") {
                            res_tx
                                .send_async(Result::MathError {
                                    route: page.route.clone(),
                                })
                                .await
                                .unwrap();
                        }

                        for href in page.links.iter() {
                            tracing::debug!("checking href {href}");

                            let url = match Url::parse(href.as_str()) {
                                Ok(url) => url,
                                Err(e) => {
                                    tracing::debug!("Failed to parse href {href}: {e}");
                                    continue;
                                }
                            };

                            if let Some(host) = url.host_str() {
                                if host == "docs.rs" {
                                    let mut segments = url.path_segments().unwrap();
                                    if let (Some(_crate_name), Some(version)) =
                                        (segments.next(), segments.next())
                                    {
                                        if version == "latest" {
                                            res_tx
                                                .send_async(Result::BadLink {
                                                    route: page.route.clone(),
                                                    href: href.clone(),
                                                    reason: "docs.rs links to latest version"
                                                        .to_string(),
                                                })
                                                .await
                                                .unwrap();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            tracing::debug!("Worker done checking routes");
        });
    }
    drop(res_tx);

    // Send tasks to workers
    let rev = &irev.rev;
    let total_routes = rev.page_routes.len();
    for route in rev.page_routes.keys() {
        task_tx
            .send_async(Task::CheckRoute {
                route: route.clone(),
            })
            .await
            .unwrap();
    }
    for page in rev.pages.values() {
        task_tx
            .send_async(Task::CheckPageLinks { page: page.clone() })
            .await
            .unwrap();
    }
    tracing::debug!("Done sending tasks");

    // Close the task channel
    drop(task_tx);

    let mut num_bad_routes = 0;
    while let Ok(result) = res_rx.recv_async().await {
        match result {
            Result::RouteChecked { route, status } => {
                if status != StatusCode::OK {
                    num_bad_routes += 1;
                    ms.send(RouteResult {
                        url: AbsoluteUrl::new(route.to_string()),
                        status: status.as_u16(),
                    })
                    .await;
                }
            }
            Result::BadLink {
                route,
                href,
                reason,
            } => {
                ms.send(BadLink {
                    route,
                    href,
                    reason,
                })
                .await;
            }
            Result::MathError { route } => {
                ms.send(MathError { route }).await;
            }
        }
    }
    tracing::debug!("Done receiving results!");

    ms.info(format!(
        "Validated {total_routes} routes, {num_bad_routes} bad routes"
    ))
    .await;

    let num_errors = num_bad_routes;

    ms.send(ValidationComplete { num_errors }).await;

    sock.flush().await.unwrap();

    loop {
        match sock.recv().await {
            Some(Ok(message)) => {
                if let Ok(text) = message.to_text() {
                    tracing::debug!("Received message: {}", text);
                }
            }
            Some(Err(e)) => {
                tracing::debug!("Error receiving message: {}", e);
                break;
            }
            None => {
                tracing::debug!("WebSocket connection closed");
                break;
            }
        }
    }
}
