use std::sync::Arc;

use axum::extract::ws;

use crate::impls::{cub_req::CubReqImpl, types::CubTenantImpl};

pub(crate) async fn serve_ws(
    ws: axum::extract::WebSocketUpgrade,
    tr: CubReqImpl,
) -> impl axum::response::IntoResponse {
    let ts = tr.tenant.clone();
    ws.on_upgrade(move |ws| handle_socket(ws, ts))
}

async fn handle_socket(mut socket: ws::WebSocket, ts: Arc<CubTenantImpl>) {
    let mut bx = ts.bx_rev.subscribe();

    loop {
        tokio::select! {
            Ok(event) = bx.recv() => {
                let msg = axum::extract::ws::Message::Text(merde::json::to_string(&event).unwrap());
                if let Err(e) = socket.send(msg).await {
                    tracing::error!("Failed to send WebSocket message: {}", e);
                    break;
                }
            }
            Some(result) = socket.recv() => {
                match result {
                    Ok(_) => {
                        // Ignore received messages
                        tracing::debug!("Received message from WebSocket (ignored)");
                    }
                    Err(e) => {
                        tracing::error!("Error receiving WebSocket message: {}", e);
                        break;
                    }
                }
            }
            else => break,
        }
    }
}
