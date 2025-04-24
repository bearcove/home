use conflux::{Dimensions, MediaKind, MediaProps, VCodec};
use eyre::eyre;
use ffmpeg_sidecar::{
    child::FfmpegChild,
    command::FfmpegCommand,
    event::{FfmpegEvent, LogLevel},
};
use futures_util::Stream;
use image_types::{ICodec, IntrinsicPixels, PixelDensity};
use nix::{
    sys::signal::{self, Signal},
    unistd::Pid,
};
use std::{
    path::Path,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use super::{deriver::FfmpegEncodePermit, ffmpeg::configure_ffmpeg_command};
use crate::impls::ffmpeg::parse_ffmpeg_timestamp;
use mom_types::media_types::{TargetFormat, TranscodingProgress};

#[derive(Debug)]
pub enum DetailedTranscodeEvent {
    MediaIdentified(MediaProps),
    Progress(TranscodingProgress),
    Log { level: LogLevel, message: String },
    Done,
    Error(String),
}

pub struct FFmpegTranscode {
    events: Pin<Box<ReceiverStream<DetailedTranscodeEvent>>>,
    process_id: Option<u32>,
    // we rely on its properties on drop
    _permit: FfmpegEncodePermit,
}

impl Drop for FFmpegTranscode {
    fn drop(&mut self) {
        if let Some(pid) = self.process_id.take() {
            tracing::warn!(
                "FFmpegTranscode dropped before completion, killing process {}",
                pid
            );
            // Try SIGTERM first
            if let Err(e) = signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM) {
                tracing::error!("Failed to send SIGTERM to FFmpeg process: {}", e);
                // If SIGTERM fails, try SIGKILL
                if let Err(e) = signal::kill(Pid::from_raw(pid as i32), Signal::SIGKILL) {
                    tracing::error!("Failed to send SIGKILL to FFmpeg process: {}", e);
                }
            }
        }
    }
}

impl Stream for FFmpegTranscode {
    type Item = DetailedTranscodeEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.events).poll_next(cx) {
            Poll::Ready(Some(event)) => {
                if matches!(
                    event,
                    DetailedTranscodeEvent::Done | DetailedTranscodeEvent::Error(_)
                ) {
                    self.process_id = None;
                }
                Poll::Ready(Some(event))
            }
            other => other,
        }
    }
}

impl FFmpegTranscode {
    pub fn new(
        input_path: &Path,
        output_path: &Path,
        target: TargetFormat,
        permit: FfmpegEncodePermit,
    ) -> eyre::Result<Self> {
        let mut cmd = FfmpegCommand::new();
        configure_ffmpeg_command(&mut cmd, input_path, output_path, target)?;

        // Create channel for events
        let (tx, rx) = mpsc::channel(32);
        let events = Box::pin(ReceiverStream::new(rx));

        let cmd_debug = format!("{cmd:?}");

        // Spawn FFmpeg process
        let mut child = cmd.spawn().map_err(|e| {
            tracing::error!("Failed to spawn FFmpeg process: {}", e);
            eyre!("Failed to spawn FFmpeg process: {}", e)
        })?;

        let process_id = child.as_inner().id();
        tracing::info!(
            "FFmpeg process spawned with PID: {}, input: \x1b[36m{}\x1b[0m, output: \x1b[32m{}\x1b[0m, command: \x1b[33m{}\x1b[0m",
            process_id,
            input_path.display(),
            output_path.display(),
            cmd_debug
        );

        // Spawn blocking task to handle FFmpeg events
        let tx = tx.clone();
        std::thread::spawn(move || {
            let result = handle_ffmpeg_events(child, tx);
            if let Err(e) = result {
                tracing::error!("Error handling FFmpeg events: {}", e);
            }
        });

        Ok(Self {
            events,
            process_id: Some(process_id),
            _permit: permit,
        })
    }
}

fn handle_ffmpeg_events(
    mut child: FfmpegChild,
    tx: mpsc::Sender<DetailedTranscodeEvent>,
) -> eyre::Result<()> {
    let iter = child.iter().map_err(|e| {
        tracing::error!("Failed to create iterator over FFmpeg events: {}", e);
        eyre!("Failed to create iterator over FFmpeg events: {}", e)
    })?;

    let mut had_fatal = false;
    let mut log_messages = Vec::new();

    let mut props = MediaProps {
        kind: MediaKind::Video,
        dims: Dimensions {
            w: IntrinsicPixels::from(0),
            h: IntrinsicPixels::from(0),
            density: PixelDensity::ONE,
        },
        secs: 0.0,
        ic: None,
        vp: None,
        ap: None,
    };

    for event in iter {
        let event = match event {
            FfmpegEvent::ParsedDuration(duration) => {
                props.secs = duration.duration;
                DetailedTranscodeEvent::MediaIdentified(props.clone())
            }
            FfmpegEvent::ParsedInputStream(s) => {
                uffmpeg::add_input_stream_to_media_ident(&mut props, s);

                if let Some(vc) = props.vp.as_ref() {
                    if let Some(vc) = vc.codec.as_ref() {
                        match vc.as_str() {
                            "png" | "mjpeg" | "jpeg2000" | "jpegls" | "jpegxl" | "bmp" | "tiff"
                            | "webp" | "gif" => {
                                props.kind = MediaKind::Bitmap;
                                props.vp = None;
                            }
                            _ => (),
                        }
                    }
                };

                DetailedTranscodeEvent::MediaIdentified(props.clone())
            }
            FfmpegEvent::Progress(progress) => {
                if props.secs == 0.0 && matches!(props.vc(), Some(VCodec::AV1)) {
                    props.kind = MediaKind::Bitmap;
                    props.ic = Some(ICodec::AVIF);
                    props.vp = None;
                }

                tracing::debug!("Transcoding progress: {:?}", progress);
                DetailedTranscodeEvent::Progress(TranscodingProgress {
                    frame: progress.frame,
                    fps: progress.fps,
                    quality: progress.q,
                    size_kb: progress.size_kb,
                    processed_time: {
                        let parsed_time = parse_ffmpeg_timestamp(&progress.time);
                        match parsed_time {
                            Ok(time) => time,
                            Err(_) => {
                                if let Some(video_params) = props.vp.as_ref() {
                                    if let Some(fps) = video_params.frame_rate {
                                        progress.frame as f64 / fps
                                    } else {
                                        tracing::warn!(
                                            "No frame rate available, defaulting to 0.0"
                                        );
                                        0.0
                                    }
                                } else {
                                    tracing::warn!(
                                        "No video parameters available (props = {props:?}), defaulting to 0.0"
                                    );
                                    0.0
                                }
                            }
                        }
                    },
                    total_time: props.secs, // NaN incoming!
                    bitrate_kbps: progress.bitrate_kbps,
                    speed: progress.speed,
                })
            }
            FfmpegEvent::Log(level, message) => {
                let log_entry = format!("[{level:?}] {message}");
                log_messages.push(log_entry.clone());
                match level {
                    LogLevel::Error => {}
                    LogLevel::Warning => {}
                    LogLevel::Info => {}
                    LogLevel::Fatal => {
                        had_fatal = true;
                    }
                    LogLevel::Unknown => {}
                }
                DetailedTranscodeEvent::Log { level, message }
            }
            FfmpegEvent::Error(error) => {
                if error.contains("No streams found") {
                    continue;
                }
                tracing::error!("ffmpeg-sidecar error: {}", error);
                log_messages.push(format!("[ERROR] ffmpeg-sidecar error: {error}"));
                continue;
            }
            _ => continue,
        };

        // If send fails, the receiver was dropped - time to stop
        if tx.blocking_send(event).is_err() {
            tracing::warn!("Event receiver dropped, stopping FFmpeg event processing");
            return Ok(());
        }
    }

    if had_fatal {
        let error_message = format!(
            "FFmpeg process had fatal errors. Full log:\n{}",
            log_messages.join("\n")
        );
        tx.blocking_send(DetailedTranscodeEvent::Error(error_message))?;
    } else {
        tx.blocking_send(DetailedTranscodeEvent::Done)?;
    }

    Ok(())
}
