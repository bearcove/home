// video.rs
use axum::Extension;
use axum::extract::ws;
use axum::http::StatusCode;
use eyre::eyre;
use merde::IntoStatic;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

use super::{MomTenantState, Reply, TenantExtractor};
use crate::impls::{
    deriver::{acquire_ffmpeg_encode_permit, transcode_media_data},
    site::{IntoReply, MerdeJson},
};
use mom_types::{
    TranscodeJobInfo, TranscodeParams, TranscodeResponse, TranscodeResponseAlreadyInProgress,
    TranscodeResponseDone,
    media_types::{HeadersMessage, TranscodeEvent, TranscodingCompleteMessage, WebSocketMessage},
};

#[axum::debug_handler]
pub(crate) async fn upload(
    TenantExtractor(ts): TenantExtractor,
    ws: axum::extract::WebSocketUpgrade,
) -> Reply {
    ws.on_upgrade(move |socket| handle_ws(socket, ts))
        .into_reply()
}

async fn handle_ws(mut socket: ws::WebSocket, ts: Arc<MomTenantState>) {
    if let Err(e) = handle_ws_inner(&mut socket, ts).await {
        tracing::warn!("Error in media transcode socket: {:?}", e);
        let error_message = WebSocketMessage::Error(format!("Error: {}", e));
        if let Err(send_err) = json_to_socket(&mut socket, &error_message).await {
            tracing::error!("Failed to send error message to websocket: {}", send_err);
        }
    }

    // well if we can't close gracefully, we can't close gracefully.
    _ = socket.close().await;
}

async fn handle_ws_inner(socket: &mut ws::WebSocket, _ts: Arc<MomTenantState>) -> eyre::Result<()> {
    let mut headers: Option<HeadersMessage> = None;
    let mut input_data: Vec<u8> = Vec::new();

    'read_msg: while let Some(msg) = socket.recv().await {
        let msg = msg?;
        match msg {
            ws::Message::Text(text) => {
                let message: WebSocketMessage =
                    merde::json::from_str(&text).map_err(|e| e.into_static())?;
                match message {
                    WebSocketMessage::Headers(h) => {
                        headers = Some(h);
                    }
                    WebSocketMessage::UploadDone(u) => {
                        if u.uploaded_size != input_data.len() {
                            return Err(eyre!("Uploaded size does not match input data size"));
                        }
                        break 'read_msg;
                    }
                    _ => return Err(eyre!("Unexpected message type")),
                }
            }
            ws::Message::Binary(data) => {
                input_data.extend_from_slice(&data);
            }
            ws::Message::Close(_) => {
                return Err(eyre!("WebSocket closed during upload"));
            }
            _ => {}
        }
    }

    let headers = headers.ok_or_else(|| eyre!("Headers not received"))?;

    let (tx, mut rx) = mpsc::channel(100);
    let start_time = Instant::now();
    let permit = acquire_ffmpeg_encode_permit().await;
    let elapsed = start_time.elapsed();
    tracing::info!("Time taken to acquire FFmpeg encode permit: {:?}", elapsed);
    let mut transcode_task = std::pin::pin!(transcode_media_data(
        input_data,
        headers.target_format,
        tx,
        permit
    ));

    let output_data = loop {
        tokio::select! {
            ev = rx.recv() => {
                if let Some(ev) = ev {
                    if let TranscodeEvent::Progress(progress) = &ev {
                        tracing::info!("Transcode progress: {}", progress);
                    }
                    json_to_socket(socket, &WebSocketMessage::TranscodingEvent(ev)).await?;
                } else {
                    tracing::debug!("Transcode progress channel closed");
                    break transcode_task.await?;
                }
            }
            result = &mut transcode_task => {
                break result?;
            }
        }
    };
    let output_size = output_data.len();
    json_to_socket(
        socket,
        &WebSocketMessage::TranscodingComplete(TranscodingCompleteMessage { output_size }),
    )
    .await?;

    // Send the output data in slices of maximum 2MB
    const CHUNK_SIZE: usize = 2_000_000; // 2MB
    for chunk in output_data.chunks(CHUNK_SIZE) {
        socket.send(ws::Message::Binary(chunk.to_vec())).await?;
    }

    Ok(())
}

async fn json_to_socket(
    socket: &mut ws::WebSocket,
    payload: &impl merde::Serialize,
) -> eyre::Result<()> {
    let json_string = merde::json::to_string(payload)?;
    socket.send(ws::Message::Text(json_string)).await?;
    Ok(())
}

// #[axum::debug_handler]
pub(crate) async fn transcode(
    Extension(TenantExtractor(ts)): Extension<TenantExtractor>,
    body: String,
) -> Reply {
    let params: TranscodeParams = merde::json::from_str(&body).unwrap();

    let start_time = Instant::now();
    let permit = acquire_ffmpeg_encode_permit().await;
    let elapsed = start_time.elapsed();
    tracing::info!("Time taken to acquire FFmpeg encode permit: {:?}", elapsed);

    let mut info = {
        let mut locks = ts.transcode_jobs.lock();
        if let Some(info) = locks.get(&params) {
            tracing::info!("Transcode already in progress: {info:?}");
            let mut res = MerdeJson(TranscodeResponse::AlreadyInProgress(
                TranscodeResponseAlreadyInProgress {
                    info: format!("Transcode already in progress: {info:#?}"),
                },
            ))
            .into_reply()
            .unwrap();
            *res.status_mut() = StatusCode::CONFLICT;
            return Ok(res);
        }

        let info = TranscodeJobInfo {
            started: Instant::now(),
            last_ping: Instant::now(),
            last_progress: None,
        };
        locks.insert(params.clone(), info.clone());
        info
    };

    struct RemoveOnDrop {
        ts: Arc<MomTenantState>,
        params: TranscodeParams,
    }
    impl Drop for RemoveOnDrop {
        fn drop(&mut self) {
            let mut locks = self.ts.transcode_jobs.lock();
            locks.remove(&self.params);
            tracing::info!("Removed transcode job for params: {:?}", self.params);
        }
    }
    let _guard = RemoveOnDrop {
        ts: ts.clone(),
        params: params.clone(),
    };

    // Read the input file from object storage
    let input_data = ts
        .object_store
        .get(&params.input)
        .await
        .map_err(|e| eyre!("Failed to read input file from object storage: {}", e))?;

    // Create a channel for progress updates
    let (tx, mut rx) = mpsc::channel(100);

    // Start the transcoding task
    let input_bytes = input_data.bytes().await?.to_vec();
    let mut transcode_task = std::pin::pin!(transcode_media_data(
        input_bytes,
        params.target_format,
        tx,
        permit
    ));

    let output_data = loop {
        tokio::select! {
            ev = rx.recv() => {
                if let Some(ev) = ev {
                    if let TranscodeEvent::Progress(progress) = ev {
                        tracing::info!("Transcode progress: {progress}");
                        info.last_ping = Instant::now();
                        info.last_progress = Some(progress);

                        {
                            let mut locks = ts.transcode_jobs.lock();
                            locks.insert(params.clone(), info.clone());
                        }
                    }
                } else {
                    tracing::debug!("Transcode progress channel closed");
                    break transcode_task.await?;
                }
            }
            result = &mut transcode_task => {
                break result?;
            }
        }
    };

    // Calculate the output size
    let output_size = output_data.len();

    // Write the output file to object storage
    ts.object_store
        .put(&params.output, output_data.into())
        .await
        .map_err(|e| eyre!("Failed to write output file to object storage: {}", e))?;

    // Create a response
    let response = TranscodeResponse::Done(TranscodeResponseDone { output_size });

    MerdeJson(response).into_reply()
}
