use camino::Utf8PathBuf;
use eyre::eyre;
pub use ffmpeg_sidecar;

use conflux::{Dimensions, MediaKind, MediaProps, VParams};
use ffmpeg_sidecar::{
    event::{Stream, StreamTypeSpecificData},
    metadata::FfmpegMetadata,
};
use image_types::{ICodec, IntrinsicPixels, PixelDensity};
use tokio::task::spawn_blocking;

pub fn ffmpeg_metadata_to_media_props(meta: FfmpegMetadata) -> MediaProps {
    let mut props = MediaProps::new(
        MediaKind::Video,
        Dimensions {
            w: IntrinsicPixels::from(1),
            h: IntrinsicPixels::from(1),
            density: PixelDensity::ONE,
        },
        meta.duration().unwrap_or(1.0),
    );

    for stream in meta.input_streams {
        add_input_stream_to_media_ident(&mut props, stream);
    }

    if let (Some(vp), None) = (props.vp.as_ref(), props.ap.as_ref()) {
        if let Some(vc) = vp.codec.as_ref() {
            if let Some(ic) = ICodec::from_ffmpeg_codec_name(vc.as_str()) {
                props.vp = None;
                props.kind = MediaKind::Bitmap;
                props.ic = Some(ic);
            }
        }
    }

    props
}

pub fn add_input_stream_to_media_ident(props: &mut MediaProps, s: Stream) {
    match s.type_specific_data {
        StreamTypeSpecificData::Video(video) => {
            props.dims = Dimensions {
                w: IntrinsicPixels::from(video.width),
                h: IntrinsicPixels::from(video.height),
                // assuming all videos are @1x — but they're not though. CleanShot just
                // doesn't save them with a @2x suffix?
                density: PixelDensity::ONE,
            };
            props.vp = Some(VParams {
                frame_rate: Some(video.fps as f64),
                codec: Some(s.format.into()),
                pix_fmt: Some(video.pix_fmt.into()),
            });
        }
        StreamTypeSpecificData::Audio(audio) => {
            props.ap = Some(conflux::AParams {
                codec: Some(s.format.into()),
                sample_rate: Some(audio.sample_rate),
                channels: Some(audio.channels.into()),
            });
        }
        _ => {}
    }
}

pub async fn gather_ffmpeg_meta(disk_path: Utf8PathBuf) -> eyre::Result<FfmpegMetadata> {
    spawn_blocking(move || {
        let mut cmd = ffmpeg_sidecar::command::FfmpegCommand::new();
        cmd.input(disk_path);
        cmd.format("null");
        cmd.output("-");

        cmd.spawn()
            .map_err(|e| eyre!("failed to spawn ffmpeg: {e:?}"))?
            .iter()
            .map_err(|e| eyre!("failed to iterate ffmpeg output: {e:?}"))?
            .collect_metadata()
            .map_err(|e| eyre!("failed to collect ffmpeg metadata: {e:?}"))
    })
    .await
    .map_err(|e| eyre!("failed to join ffmpeg task: {e:?}"))?
}
