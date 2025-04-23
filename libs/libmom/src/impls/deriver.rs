use config::Environment;
use conflux::{DerivationKind, VCodec, VContainer};
use derivations::DerivationInfo;
use eyre::{Context as _, eyre};
use image::ICodec;
use std::{sync::Arc, time::Instant};
use svg::{DrawioToSvgOptions, SvgCleanupOptions};
use tempfile::TempDir;
use tokio::sync::{SemaphorePermit, mpsc};
use tokio_stream::StreamExt as _;

use crate::{
    DeriveJobInfo, DeriveParams, DeriveResponse, DeriveResponseAlreadyInProgress,
    DeriveResponseDone, DeriveResponseTooManyRequests,
    impls::{
        MomTenantState,
        site::{IntoReply, MerdeJson, Reply},
    },
    media_types::{TargetFormat, TranscodeEvent},
};

use super::ffmpeg_stream::{DetailedTranscodeEvent, FFmpegTranscode};

pub async fn do_derive(ts: Arc<MomTenantState>, params: DeriveParams) -> Reply {
    let mut info = {
        let mut locks = ts.derive_jobs.lock();
        if let Some(info) = locks.get(&params) {
            tracing::info!("Derive already in progress: {info:?}");
            let mut res = MerdeJson(DeriveResponse::AlreadyInProgress(
                DeriveResponseAlreadyInProgress {
                    info: format!("derive already in progress: {info:#?}"),
                },
            ))
            .into_reply()
            .unwrap();
            *res.status_mut() = axum::http::StatusCode::CONFLICT;
            return Ok(res);
        }

        let info = DeriveJobInfo {
            started: Instant::now(),
            last_ping: Instant::now(),
            last_progress: None,
        };
        locks.insert(params.clone(), info.clone());
        info
    };

    struct RemoveOnDrop {
        ts: Arc<MomTenantState>,
        params: DeriveParams,
    }
    impl Drop for RemoveOnDrop {
        fn drop(&mut self) {
            let mut locks = self.ts.derive_jobs.lock();
            locks.remove(&self.params);
            tracing::debug!(
                "Removed derivation job for \x1b[32m{:?}\x1b[0m on \x1b[36m{}\x1b[0m",
                self.params.derivation.kind,
                self.params.input.key()
            );
        }
    }
    let _guard = RemoveOnDrop {
        ts: ts.clone(),
        params: params.clone(),
    };

    let broadcast_info = {
        let ts = ts.clone();
        let params = params.clone();
        move |info: &DeriveJobInfo| {
            let mut locks = ts.derive_jobs.lock();
            locks.insert(params.clone(), info.clone());
            drop(locks);
        }
    };

    let DeriveParams { input, derivation } = params;

    let before_load = Instant::now();

    // Read the input file from object storage
    let input_key = input.key();
    let input_data = ts.object_store.get(&input_key).await.map_err(|e| {
        eyre!(
            "Failed to read input file {input_key} from object storage: {e} (storage = {:?})",
            ts.object_store.desc()
        )
    })?;

    let input_bytes = input_data.bytes().await?.to_vec();

    let load_duration = before_load.elapsed();
    let before_derive = Instant::now();

    let output_data = match &derivation.kind {
        DerivationKind::Passthrough(_) | DerivationKind::Identity(_) => input_bytes,
        DerivationKind::Bitmap(bitmap) => {
            let input_codec = ICodec::try_from(input.content_type)?;
            // Transcode image using image module
            image::load().transcode(&input_bytes, input_codec, bitmap.ic, bitmap.width)?
        }
        DerivationKind::Video(video) => {
            // Transcode video
            let (tx, mut rx) = mpsc::channel(100);
            // TODO: it might make sense to simplify this at some point — maybe get rid of `TargetFormat`, not sure.
            let target_format = match (video.container, video.vc) {
                (VContainer::WebM, VCodec::VP9) => TargetFormat::VP9,
                (VContainer::MP4, VCodec::AV1) => TargetFormat::AV1,
                (container, vc) => {
                    return Err(eyre!(
                        "Unsupported video container/codec combination: {container:?}/{vc:?}"
                    )
                    .into());
                }
            };

            let permit = match acquire_permit_or_429() {
                Ok(value) => value,
                Err(value) => return value,
            };
            let mut transcode_task =
                std::pin::pin!(transcode_media_data(input_bytes, target_format, tx, permit));

            loop {
                tokio::select! {
                    ev = rx.recv() => {
                        if let Some(TranscodeEvent::Progress(progress)) = ev {
                            tracing::info!("Transcode progress: {progress}");
                            info.last_ping = Instant::now();
                            info.last_progress = Some(progress);
                            broadcast_info(&info);
                        } else {
                            tracing::debug!("Transcode progress channel closed");
                            break transcode_task.await?;
                        }
                    }
                    result = &mut transcode_task => {
                        break result?;
                    }
                }
            }
        }
        DerivationKind::VideoThumbnail(thumb) => {
            // Extract thumbnail from video
            let (tx, mut rx) = mpsc::channel(100);

            let target_format = match thumb.ic {
                ICodec::JXL => TargetFormat::ThumbJXL,
                ICodec::AVIF => TargetFormat::ThumbAVIF,
                ICodec::WEBP => TargetFormat::ThumbWEBP,
                other => return Err(eyre!("Unsupported thumbnail codec: {other:?}").into()),
            };
            let permit = match acquire_permit_or_429() {
                Ok(value) => value,
                Err(value) => return value,
            };
            let mut transcode_task =
                std::pin::pin!(transcode_media_data(input_bytes, target_format, tx, permit));

            loop {
                tokio::select! {
                    ev = rx.recv() => {
                        if let Some(TranscodeEvent::Progress(progress)) = ev {
                            tracing::info!("Transcode progress: {progress}");
                            info.last_ping = Instant::now();
                            info.last_progress = Some(progress);
                            broadcast_info(&info);
                        } else {
                            tracing::debug!("Transcode progress channel closed");
                            break transcode_task.await?;
                        }
                    }
                    result = &mut transcode_task => {
                        break result?;
                    }
                }
            }
        }
        DerivationKind::DrawioRender(d) => {
            // Convert drawio to SVG
            let svg = svg::load();
            let svg_data = svg
                .drawio_to_svg(input_bytes.into(), DrawioToSvgOptions { minify: true })
                .await?;
            svg.inject_font_faces(&svg_data, &d.svg_font_face_collection)
                .await?
        }
        DerivationKind::SvgCleanup(_) => {
            let svg = svg::load();
            svg.cleanup_svg(&input_bytes[..], SvgCleanupOptions {})?
        }
    };

    let derive_duration = before_derive.elapsed();

    let output_size = output_data.len();
    let dinfo = DerivationInfo::new(&input, &derivation);
    let dest_key = dinfo.key(Environment::default());

    let before_write = Instant::now();

    // Write output to object storage
    ts.object_store
        .put(&dest_key, output_data.into())
        .await
        .map_err(|e| eyre!("Failed to write output file to object storage: {}", e))?;

    let write_duration = before_write.elapsed();
    let input_type = dinfo.input.content_type;
    let output_description = &dinfo.derivation.kind;
    tracing::info!(
        "\x1b[36m{} => {}\x1b[0m took \x1b[32m{:?}\x1b[0m (load={:?}, derive={:?}, write={:?}) (\x1b[34m{}\x1b[0m => \x1b[34m{}\x1b[0m, e.g. \x1b[35m{:.2}x\x1b[0m)",
        input_type,
        output_description,
        load_duration + derive_duration + write_duration,
        load_duration,
        derive_duration,
        write_duration,
        bytesize::to_string(input.size, true),
        bytesize::to_string(output_size as u64, true),
        output_size as f64 / input.size as f64
    );

    // Return success response
    let response = DeriveResponse::Done(DeriveResponseDone {
        output_size,
        // this lets the cube check whether mom and it agree on the output key
        dest: dest_key,
    });

    MerdeJson(response).into_reply()
}

#[allow(clippy::result_large_err)]
fn acquire_permit_or_429() -> Result<FfmpegEncodePermit, Reply> {
    Ok(match try_acquire_ffmpeg_encode_permit() {
        Some(permit) => permit,
        None => {
            return Err(MerdeJson(DeriveResponse::TooManyRequests(
                DeriveResponseTooManyRequests {},
            ))
            .into_reply());
        }
    })
}

// FIXME: don't hardcode that limit?
const MAX_CONCURRENT_FFMPEG_TRANSCODES: usize = 64;
static VIDEO_TRANSCODE_SEMAPHORE: tokio::sync::Semaphore =
    tokio::sync::Semaphore::const_new(MAX_CONCURRENT_FFMPEG_TRANSCODES);

pub struct FfmpegEncodePermit {
    _permit: SemaphorePermit<'static>,
}

impl From<SemaphorePermit<'static>> for FfmpegEncodePermit {
    fn from(_permit: SemaphorePermit<'static>) -> Self {
        FfmpegEncodePermit { _permit }
    }
}

/// Acquires an FFmpeg encode permit asynchronously.
///
/// Returns an `FfmpegEncodePermit` when acquired.
pub async fn acquire_ffmpeg_encode_permit() -> FfmpegEncodePermit {
    VIDEO_TRANSCODE_SEMAPHORE.acquire().await.unwrap().into()
}

/// Tries to acquire an FFmpeg encode permit immediately.
///
/// Returns `Some(FfmpegEncodePermit)` if successful, `None` if no permit is available.
pub fn try_acquire_ffmpeg_encode_permit() -> Option<FfmpegEncodePermit> {
    VIDEO_TRANSCODE_SEMAPHORE
        .try_acquire()
        .ok()
        .map(|permit| permit.into())
}

pub async fn transcode_media_data(
    input_data: Vec<u8>,
    target_format: TargetFormat,
    tx: mpsc::Sender<TranscodeEvent>,
    permit: FfmpegEncodePermit,
) -> eyre::Result<Vec<u8>> {
    let temp_dir = TempDir::new()?;
    let input_path = temp_dir.path().join("input");
    let output_path = temp_dir
        .path()
        .join(format!("output.{}", target_format.ffmpeg_output_ext()));

    tokio::fs::write(&input_path, &input_data).await?;

    transcode_media(
        input_path.clone(),
        output_path.clone(),
        target_format,
        tx,
        permit,
    )
    .await
    .wrap_err_with(|| {
        format!(
            "Error while transcoding video file (size: {} bytes) to {:?} format. Input path: {:?}, Output path: {:?}",
            input_data.len(),
            target_format,
            input_path,
            output_path
        )
    })?;

    let output_data = tokio::fs::read(&output_path).await?;
    Ok(output_data)
}

async fn transcode_media(
    input_path: std::path::PathBuf,
    output_path: std::path::PathBuf,
    target_format: TargetFormat,
    tx: mpsc::Sender<TranscodeEvent>,
    permit: FfmpegEncodePermit,
) -> eyre::Result<()> {
    let mut transcode = FFmpegTranscode::new(&input_path, &output_path, target_format, permit)?;

    // Process transcoding events
    while let Some(event) = transcode.next().await {
        match event {
            DetailedTranscodeEvent::MediaIdentified(media_id) => {
                if let Err(e) = tx.send(TranscodeEvent::MediaIdentified(media_id)).await {
                    return Err(eyre!("Error sending media identification: {}", e));
                }
            }
            DetailedTranscodeEvent::Progress(progress) => {
                if let Err(e) = tx.send(TranscodeEvent::Progress(progress)).await {
                    return Err(eyre!("Error sending progress update: {}", e));
                }
            }
            DetailedTranscodeEvent::Done => {
                if let Some(postprocess) = target_format.postprocess() {
                    let input_payload = tokio::fs::read(&output_path).await?;
                    let image = image::load();
                    let output_payload = image.transcode(
                        &input_payload[..],
                        postprocess.src_ic,
                        postprocess.dst_ic,
                        None,
                    )?;
                    // it's kind of wasteful to write this back to disk, but that's the way it is right now.
                    tokio::fs::write(&output_path, output_payload).await?;
                }
                return Ok(());
            }
            DetailedTranscodeEvent::Error(error) => {
                tracing::error!("FFmpeg transcoding error: {}", error);
                continue;
            }
            DetailedTranscodeEvent::Log { level, message } => {
                // Log but don't send to client unless it's an error
                match level {
                    ffmpeg_sidecar::event::LogLevel::Error
                    | ffmpeg_sidecar::event::LogLevel::Fatal => {
                        return Err(eyre!("FFmpeg error: {}", message));
                    }
                    _ => {
                        // ignore
                    }
                }
            }
        }
    }

    Err(eyre!("Transcoding process ended unexpectedly"))
}
