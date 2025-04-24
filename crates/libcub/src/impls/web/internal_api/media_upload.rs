use std::sync::Arc;

use axum::extract::ws;
use camino::{Utf8Path, Utf8PathBuf};
use config_types::is_development;
use conflux::{InputPath, MediaProps, PathMappings};
use cub_types::CubTenant;
use eyre::eyre;
use futures_core::future::BoxFuture;
use image_types::ICodec;
use libmomclient::TranscodingEventListener;
use merde::IntoStatic;
use mom_types::media_types::{self, TargetFormat, TranscodingProgress};
use tempfile::TempDir;
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::mpsc,
};
use uffmpeg::gather_ffmpeg_meta;

use crate::impls::{
    CubTenantImpl,
    cub_req::CubReqImpl,
    reply::{IntoLegacyReply, LegacyReply},
};

#[derive(Debug)]
enum WebSocketMessage {
    Headers(HeadersMessage),
    Commit(CommitMessage),
    UploadDone(UploadDoneMessage),
    MediaIdentified(MediaProps),
    ConversionProgress(TranscodingProgress),
    ConversionDone(ConversionDoneMessage),
    ActionDone(ActionDoneMessage),
    Error(String),
}

merde::derive! {
    impl (Serialize, Deserialize) for enum WebSocketMessage
    externally_tagged {
        "Headers" => Headers,
        "Commit" => Commit,
        "UploadDone" => UploadDone,
        "MediaIdentified" => MediaIdentified,
        "ConversionProgress" => ConversionProgress,
        "ConversionDone" => ConversionDone,
        "ActionDone" => ActionDone,
        "Error" => Error,
    }
}

#[derive(Debug)]
struct HeadersMessage {
    page_input_path: InputPath,
    file_name: String,
    file_size: u64,
    action: Action,
    paragraph_byte_offset: u64,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct HeadersMessage {
        page_input_path,
        file_name,
        file_size,
        action,
        paragraph_byte_offset
    }
}

#[derive(Debug)]
struct UploadDoneMessage {
    uploaded_size: u64,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct UploadDoneMessage { uploaded_size }
}
#[derive(Debug)]
struct CommitMessage {
    name: String,
    /// displayed after the image (if it's a figure) or on hover
    title: String,
    /// describes the content of the image, for non-sighted users
    alt: String,
    is_figure: bool,
    attr: Option<String>,
    attrlink: Option<String>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct CommitMessage {
        name,
        title,
        alt,
        is_figure,
        attr,
        attrlink
    }
}

#[derive(Debug)]
struct ConversionDoneMessage {
    file_size: u64,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct ConversionDoneMessage { file_size }
}

#[derive(Debug)]
struct ActionDoneMessage {
    done: bool,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct ActionDoneMessage { done }
}

#[derive(Debug)]
enum Action {
    Replace,
    Append,
    SetThumb,
}

merde::derive! {
    impl (Serialize, Deserialize) for enum Action string_like {
        "replace" => Replace,
        "append" => Append,
        "set_thumb" => SetThumb,
    }
}

pub(crate) async fn serve_media_upload(
    ws: axum::extract::WebSocketUpgrade,
    tr: CubReqImpl,
) -> LegacyReply {
    let ts = tr.tenant.clone();
    ws.on_upgrade(move |socket| handle_ws(socket, ts))
        .into_legacy_reply()
}

async fn handle_ws(mut socket: ws::WebSocket, ts: Arc<CubTenantImpl>) {
    if let Err(e) = handle_ws_inner(&mut socket, ts).await {
        tracing::warn!("Error in image upload socket: {:?}", e);
        let error_message = WebSocketMessage::Error(format!("Error: {e}"));
        if let Err(send_err) = json_to_socket(&mut socket, &error_message).await {
            tracing::error!("Failed to send error message to websocket: {}", send_err);
        }
    }
}

async fn handle_ws_inner(
    socket: &mut ws::WebSocket,
    tenant: Arc<CubTenantImpl>,
) -> eyre::Result<()> {
    if !is_development() {
        tracing::debug!("Image upload attempted in non-development mode");
        return Err(eyre!("Image upload is only available in development mode"));
    }

    let mut headers: Option<HeadersMessage> = None;

    let temp_dir = TempDir::new()?;
    let input_path = temp_dir.path().join("home-media-upload");
    let mut input_len: usize = 0;
    let mut file = tokio::fs::File::create(&input_path).await?;

    'read_msg: while let Some(msg) = socket.recv().await {
        let msg = msg?;
        match msg {
            ws::Message::Text(text) => {
                let message: WebSocketMessage =
                    merde::json::from_str(&text).map_err(|e| e.into_static())?;

                match message {
                    WebSocketMessage::Headers(h) => {
                        tracing::info!("Received headers: {:?}", h);
                        headers = Some(h);

                        if std::env::var("ACTIVATE_SAFARI").is_ok() {
                            if Command::new("osascript")
                                .arg("-e")
                                .arg("tell application \"Safari\" to activate")
                                .spawn()
                                .is_err()
                            {
                                tracing::debug!("Failed to activate Safari");
                            } else {
                                tracing::info!("Activated Safari");
                            }
                        }
                    }
                    WebSocketMessage::UploadDone(u) => {
                        tracing::debug!("Received upload done: {:?}", u);
                        if u.uploaded_size != input_len as u64 {
                            return Err(eyre!("Uploaded size does not match received bytes"));
                        }
                        break 'read_msg;
                    }
                    _ => return Err(eyre!("Unexpected message type during upload: {message:?}")),
                }
            }
            ws::Message::Binary(data) => {
                tracing::debug!(
                    "Received binary data of length: {}, total bytes received: {}",
                    data.len(),
                    input_len + data.len()
                );
                input_len += data.len();
                file.write_all(&data).await?;
            }
            ws::Message::Close(_) => {
                tracing::info!("WebSocket gracefully closed");
                return Ok(());
            }
            _ => {}
        }
    }

    let headers = headers.ok_or_else(|| eyre!("Headers not received"))?;
    tracing::debug!("Processing image upload with headers: {:?}", headers,);
    if input_len != headers.file_size as usize {
        return Err(eyre!(
            "Received file size ({}) does not match expected size ({})",
            input_len,
            headers.file_size
        ));
    }

    let irev = tenant.rev()?;
    let page = irev
        .rev
        .pages
        .get(&headers.page_input_path)
        .ok_or_else(|| eyre!("Page not found: {}", headers.page_input_path))?;

    let mappings = PathMappings::from_ti(tenant.ti().as_ref());
    let page_disk_path = mappings.to_disk_path(&page.path)?;
    let page_dir = page_dir(&page_disk_path)?;

    let media_type = if headers.file_name.ends_with(".mp4")
        || headers.file_name.ends_with(".mov")
        || headers.file_name.ends_with(".mkv")
        || headers.file_name.ends_with(".webm")
        || headers.file_name.ends_with(".avi")
        || headers.file_name.ends_with(".wmv")
    {
        MediaType::Video
    } else if headers.file_name.ends_with(".svg") {
        MediaType::Diagram
    } else {
        MediaType::Image
    };

    let output_filename = format!("output{}", media_type.output_extension());
    let temp_output_path = temp_dir.path().join(output_filename);

    tracing::debug!("Preparing to run {media_type} conversion command");
    let start_time = std::time::Instant::now();

    match media_type {
        MediaType::Diagram => {
            // nothing to change!
            tokio::fs::copy(&input_path, &temp_output_path).await?;
        }
        MediaType::Image => {
            // first poke the image with ffprobe
            let meta =
                gather_ffmpeg_meta(Utf8PathBuf::try_from(input_path.to_owned()).unwrap()).await?;
            let props = uffmpeg::ffmpeg_metadata_to_media_props(meta);
            let src_ic = match props.ic.as_ref() {
                Some(ic) => *ic,
                None => {
                    tracing::warn!("Unknown image codec, raw ffmpeg metadata: {:?}", props);
                    return Err(eyre!("Unknown image codec"));
                }
            };
            json_to_socket(socket, &WebSocketMessage::MediaIdentified(props)).await?;

            let image = libimage::load();
            let input_bytes = tokio::fs::read(&input_path).await?;
            let output_bytes = image.transcode(&input_bytes, src_ic, ICodec::JXL, None)?;
            tokio::fs::write(&temp_output_path, output_bytes).await?;
        }
        MediaType::Video => {
            struct EventListener {
                tx: mpsc::Sender<media_types::TranscodeEvent>,
            }

            impl TranscodingEventListener for EventListener {
                fn on_transcoding_event(
                    &self,
                    ev: media_types::TranscodeEvent,
                ) -> BoxFuture<'_, libmomclient::Result<()>> {
                    Box::pin(async move {
                        _ = self.tx.send(ev).await;
                        Ok(())
                    })
                }
            }

            let (ev_tx, mut ev_rx) = mpsc::channel::<media_types::TranscodeEvent>(32);
            let relay_progress_fut = async {
                while let Some(ev) = ev_rx.recv().await {
                    match ev {
                        media_types::TranscodeEvent::MediaIdentified(media_identification) => {
                            json_to_socket(
                                socket,
                                &WebSocketMessage::MediaIdentified(media_identification),
                            )
                            .await?;
                        }
                        media_types::TranscodeEvent::Progress(transcoding_progress) => {
                            json_to_socket(
                                socket,
                                &WebSocketMessage::ConversionProgress(transcoding_progress),
                            )
                            .await?;
                        }
                    }
                }
                Ok::<_, eyre::Report>(())
            };

            let conver_fut = async {
                let mut uploader = tenant
                    .tcli()
                    .media_uploader(Box::new(EventListener { tx: ev_tx }))
                    .await?;

                uploader
                    .with_headers(media_types::HeadersMessage {
                        target_format: TargetFormat::AV1,
                        file_name: headers.file_name,
                        file_size: input_len,
                    })
                    .await?;

                let file = tokio::fs::File::open(&input_path).await?;
                let mut reader = tokio::io::BufReader::new(file);
                let mut buffer = vec![0; 2 * 1024 * 1024]; // 2MB buffer
                loop {
                    let bytes_read = reader.read(&mut buffer).await?;
                    if bytes_read == 0 {
                        break;
                    }
                    tracing::info!("Uploading chunk of {} bytes", bytes_read);
                    uploader
                        .upload_chunk(buffer[..bytes_read].to_vec().into())
                        .await?;
                }

                struct ChunkReceiver {
                    file: tokio::fs::File,
                }

                impl libmomclient::ChunkReceiver for ChunkReceiver {
                    fn on_chunk(
                        &mut self,
                        chunk: Vec<u8>,
                    ) -> futures_core::future::BoxFuture<'_, eyre::Result<()>> {
                        Box::pin(async move {
                            tracing::info!("Received chunk of size {} bytes", chunk.len());
                            self.file.write_all(&chunk).await?;
                            self.file.sync_all().await?;
                            tracing::info!("Chunk written and synced to file");
                            Ok(())
                        })
                    }
                }

                let receiver = ChunkReceiver {
                    file: tokio::fs::File::create(&temp_output_path).await?,
                };

                tracing::info!("Starting to download and write video chunks");
                uploader
                    .done_and_download_result(input_len, Box::new(receiver))
                    .await?;

                Ok(())
            };

            tracing::info!("Video download and writing completed");
            tokio::try_join!(relay_progress_fut, conver_fut)?;
        }
    };

    let duration = start_time.elapsed();
    tracing::debug!("{media_type} conversion completed in {duration:?}");
    let result_size = fs::metadata(&temp_output_path).await?.len();

    tracing::debug!("{media_type} conversion successful, sending ConversionDone message");
    json_to_socket(
        socket,
        &WebSocketMessage::ConversionDone(ConversionDoneMessage {
            file_size: result_size,
        }),
    )
    .await?;

    let commit = loop {
        match socket.recv().await {
            Some(Ok(ws::Message::Text(text))) => {
                tracing::debug!("Received text message: {}", text);
                let message: WebSocketMessage =
                    merde::json::from_str(&text).map_err(|e| e.into_static())?;
                if let WebSocketMessage::Commit(c) = message {
                    tracing::debug!("Received commit: {:?}", c);
                    break c;
                }
            }
            Some(Ok(ws::Message::Close(_))) => {
                tracing::debug!("Received close message");
                return Err(eyre!("WebSocket closed before receiving Commit message"));
            }
            Some(Err(e)) => {
                tracing::error!("Error receiving WebSocket message: {:?}", e);
                return Err(eyre!("WebSocket error: {}", e));
            }
            None => {
                return Err(eyre!("WebSocket connection closed unexpectedly"));
            }
            other => {
                tracing::error!("Unexpected WebSocket message: {:?}", other);
                return Err(eyre!("Unexpected WebSocket message"));
            }
        }
    };

    let is_2x = commit.name.contains("@2x.");
    let suffix = if is_2x { "@2x" } else { "" };
    let name = &commit.name;
    let extension = media_type.output_extension();
    let final_image_name = format!("{name}{suffix}{extension}");
    let output_path = page_dir.join(&final_image_name);

    tracing::debug!(
        "Moving converted image to final location: {:?}",
        output_path
    );
    fs::create_dir_all(&page_dir).await?;
    fs::rename(temp_output_path, &output_path).await?;

    let mut lines = vec![];
    lines.push("+++".to_string());
    lines.push(if commit.is_figure {
        ":figure:".to_string()
    } else {
        ":media:".to_string()
    });
    lines.push(format!("    src: {final_image_name}"));
    if !commit.title.is_empty() {
        lines.push("    title: |".to_string());
        for line in commit.title.lines() {
            lines.push(format!("        {line}"));
        }
    }
    if !commit.alt.is_empty() {
        lines.push("    alt: |".to_string());
        for line in commit.alt.lines() {
            lines.push(format!("        {line}"));
        }
    }
    if let Some(attr) = commit.attr {
        lines.push("    attr: |".to_string());
        for line in attr.lines() {
            lines.push(format!("        {line}"));
        }
    }
    if let Some(attrlink) = commit.attrlink {
        lines.push("    attrlink: |".to_string());
        for line in attrlink.lines() {
            lines.push(format!("        {line}"));
        }
    }
    lines.push("+++".to_string());

    let markdown = lines.join("\n");

    tracing::debug!("Reading page content");
    let mut content = fs::read_to_string(&page_disk_path).await?;

    let paragraph_start = content[..headers.paragraph_byte_offset as usize]
        .rfind("\n\n")
        .map(|i| i + 2)
        .unwrap_or(0);

    let paragraph_end = content[headers.paragraph_byte_offset as usize..]
        .find("\n\n")
        .map(|i| i + headers.paragraph_byte_offset as usize)
        .unwrap_or(content.len());

    tracing::debug!("Updating page content with new image markdown");
    match headers.action {
        Action::Append => {
            content.insert_str(paragraph_end, &format!("\n\n{markdown}"));
        }
        Action::Replace => {
            content.replace_range(paragraph_start..paragraph_end, &markdown);
        }
        Action::SetThumb => {
            if content.ends_with("\n\n") {
                content.pop();
            } else {
                content.push('\n');
            }
        }
    }

    tracing::debug!("Writing updated page content");
    fs::write(&page_disk_path, content).await?;

    tracing::debug!("Sending ActionDone message");
    json_to_socket(
        socket,
        &WebSocketMessage::ActionDone(ActionDoneMessage { done: true }),
    )
    .await?;

    tracing::debug!("Image upload process completed successfully");
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

#[derive(Debug)]
enum MediaType {
    Image,
    Diagram,
    Video,
}

impl std::fmt::Display for MediaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MediaType::Image => write!(f, "image"),
            MediaType::Video => write!(f, "video"),
            MediaType::Diagram => write!(f, "diagram"),
        }
    }
}

impl MediaType {
    fn output_extension(&self) -> &'static str {
        match self {
            MediaType::Image => ".jxl",
            MediaType::Video => ".mp4",
            MediaType::Diagram => ".svg",
        }
    }
}

fn page_dir(page_disk_path: &Utf8Path) -> eyre::Result<Utf8PathBuf> {
    if page_disk_path.ends_with("_index.md") {
        Ok(page_disk_path.parent().unwrap().to_owned())
    } else {
        let file_stem = page_disk_path.file_stem().unwrap();
        Ok(page_disk_path.parent().unwrap().join(file_stem))
    }
}
