use eyre::Context;
use ffmpeg_sidecar::command::FfmpegCommand;
use std::path::Path;

use crate::media_types::TargetFormat;

pub fn configure_ffmpeg_command(
    cmd: &mut FfmpegCommand,
    input_path: &Path,
    output_path: &Path,
    target: TargetFormat,
) -> eyre::Result<()> {
    // this breaks stream parsing?? we get duration 0 and no
    // content type if we set it, so.. let's not set it.
    // cmd.arg("-loglevel").arg("level+debug");

    // Configure input
    cmd.input(input_path.to_str().unwrap());

    // Set output container format and codec options based on target
    match target {
        TargetFormat::AV1 => {
            cmd.arg("-f")
                .arg("mp4")
                .arg("-c:v")
                .arg("libsvtav1")
                .arg("-crf")
                .arg("35")
                .arg("-preset")
                .arg("5");

            // Add common settings
            vid_common(cmd);
        }
        TargetFormat::VP9 => {
            cmd.arg("-f")
                .arg("webm")
                .arg("-c:v")
                .arg("libvpx-vp9")
                .arg("-crf")
                .arg("24")
                .arg("-b:v")
                .arg("0")
                .arg("-speed")
                .arg("4")
                .arg("-row-mt")
                .arg("1")
                .arg("-tile-columns")
                .arg("6");

            // Add common settings
            vid_common(cmd);
        }
        TargetFormat::ThumbJXL => {
            assert!(output_path.to_str().unwrap().ends_with(".jxl"));
            cmd.arg("-c:v")
                .arg("libjxl")
                .arg("-distance")
                // okay for a thumbnail
                .arg("3.5")
                .arg("-effort")
                .arg("7");
            thumb_common(cmd);
        }
        TargetFormat::ThumbAVIF | TargetFormat::ThumbWEBP => {
            assert!(output_path.to_str().unwrap().ends_with(".jxl"));
            cmd.arg("-c:v")
                .arg("libjxl")
                .arg("-lossless")
                .arg("1")
                .arg("-distance")
                .arg("0.0");
            thumb_common(cmd);
        }
    }

    // Set output path
    cmd.output(output_path.to_str().unwrap()).overwrite();

    Ok(())
}

fn vid_common(cmd: &mut FfmpegCommand) {
    cmd.arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-movflags")
        .arg("+faststart")
        .arg("-c:a")
        .arg("libopus")
        .arg("-ab")
        .arg("128k");
}

fn thumb_common(cmd: &mut FfmpegCommand) {
    cmd.arg("-update").arg("1").arg("-frames:v").arg("1");
}

// the input looks like "00:00:18.66" and should give 18.66
pub fn parse_ffmpeg_timestamp(input: &str) -> eyre::Result<f64> {
    let parts: Vec<&str> = input.split(':').collect();
    if parts.len() != 3 {
        return Err(eyre::eyre!("for input {input:?}, not three parts"));
    }

    let hours: f64 = parts[0].parse().wrap_err_with(|| {
        format!(
            "Failed to parse hours from input '{}', part '{}'",
            input, parts[0]
        )
    })?;
    let minutes: f64 = parts[1].parse().wrap_err_with(|| {
        format!(
            "Failed to parse minutes from input '{}', part '{}'",
            input, parts[1]
        )
    })?;
    let seconds: f64 = parts[2].parse().wrap_err_with(|| {
        format!(
            "Failed to parse seconds from input '{}', part '{}'",
            input, parts[2]
        )
    })?;

    Ok(hours * 3600.0 + minutes * 60.0 + seconds)
}

#[cfg(test)]
mod tests {
    use super::parse_ffmpeg_timestamp;

    #[test]
    fn test_parse_ffmpeg_timestamp() {
        assert_eq!(parse_ffmpeg_timestamp("00:00:18.66").unwrap(), 18.66);
        assert_eq!(parse_ffmpeg_timestamp("00:01:30.00").unwrap(), 90.0);
        assert_eq!(parse_ffmpeg_timestamp("01:00:00.50").unwrap(), 3600.5);
        assert_eq!(parse_ffmpeg_timestamp("10:30:45.75").unwrap(), 37845.75);
        assert_eq!(parse_ffmpeg_timestamp("00:00:00.00").unwrap(), 0.0);
        assert!(parse_ffmpeg_timestamp("invalid").is_err());
        assert!(parse_ffmpeg_timestamp("00:00:60.00").is_err());
        assert!(parse_ffmpeg_timestamp("00:60:00.00").is_err());
        assert_eq!(parse_ffmpeg_timestamp("24:00:00.00").unwrap(), 86400.0);
    }
}
