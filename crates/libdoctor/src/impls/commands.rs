use super::Check;
use super::DoctorError;
use super::FailedCheck;
use super::Gravity;
use super::RequiredBin;
use tokio::process::Command;

pub async fn check_all_commands() -> Vec<DoctorError> {
    let mut errs = Vec::new();
    for &required_bin in COMMANDS_TO_CHECK {
        if let Err(failed_check) = check_command_detailed(required_bin).await {
            errs.push(DoctorError::BinaryNotFound(failed_check));
        }
    }
    errs
}

async fn check_command_detailed(required_bin: RequiredBin) -> Result<(), FailedCheck> {
    for check in required_bin.checks.iter().copied() {
        let output = Command::new(check.command)
            .args(check.args)
            .output()
            .await
            .map_err(|e| FailedCheck {
                required_bin,
                check,
                status_code: -1,
                stderr: format!("Failed to execute {}: {}", check.command, e),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(FailedCheck {
                required_bin,
                check,
                status_code: output.status.code().unwrap_or(-1),
                stderr: stderr.to_string(),
            });
        }
    }

    Ok(())
}

const COMMANDS_TO_CHECK: &[RequiredBin] = &[
    RequiredBin {
        checks: &[Check {
            command: "pnpm",
            args: &["--version"],
        }],
        purpose: "Run the JavaScript render",
        notes: "To install pnpm, run `brew install pnpm`.",
        gravity: Gravity::Recommended,
    },
    RequiredBin {
        checks: &[Check {
            command: "ffmpeg",
            args: &["-version"],
        }],
        purpose: "Transcode videos",
        notes: "To install ffmpeg, run `brew install ffmpeg`.",
        gravity: Gravity::Needed,
    },
    RequiredBin {
        checks: &[Check {
            command: "node",
            args: &["--version"],
        }],
        purpose: "Use in conjunction with pnpm to run the vite bundler in dev",
        notes: "To install Node.js, run `brew install node`.",
        gravity: Gravity::Needed,
    },
    RequiredBin {
        checks: &[Check {
            command: "magick",
            args: &["--version"],
        }],
        purpose: "Support additional image formats and convert them to jpeg/xl on upload",
        notes: "To install ImageMagick v7 or above, run `brew install imagemagick`.",
        gravity: Gravity::Needed,
    },
    RequiredBin {
        checks: &[
            Check {
                command: "pyftsubset",
                args: &["--help"],
            },
            Check {
                command: "ttx",
                args: &["--version"],
            },
        ],
        purpose: "Font subsetting for diagrams",
        notes: "Both pyftsubset and ttx come from fonttools. Install with `brew install fonttools`.",
        gravity: Gravity::Needed,
    },
    RequiredBin {
        checks: &[Check {
            command: "home-drawio",
            args: &[],
        }],
        purpose: "draw.io diagram conversion to SVG",
        notes: "To install home-drawio, first add the bearcove tap with `brew tap bearcove/tap https://github.com/bearcove/tap`, then run `brew install bearcove/tap/home-drawio`.",
        gravity: Gravity::Needed,
    },
];
