use std::process::Stdio;

use bytes::Bytes;
use noteyre::bail;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

use crate::DrawioToSvgOptions;

pub async fn drawio_to_svg(input: Bytes, opts: DrawioToSvgOptions) -> noteyre::Result<Vec<u8>> {
    let mut cmd = Command::new("home-drawio");
    cmd.arg("convert");
    if opts.minify {
        cmd.arg("--minify");
    }
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn()?;

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();

    let write_future = async {
        stdin.write_all(&input).await?;
        stdin.shutdown().await?;
        drop(stdin);
        Ok::<_, std::io::Error>(())
    };

    let mut output = Vec::new();
    let read_future = async {
        stdout.read_to_end(&mut output).await?;
        Ok::<_, std::io::Error>(output)
    };

    let stderr_future = async {
        let mut stderr_output = String::new();
        stderr.read_to_string(&mut stderr_output).await?;
        Ok::<_, std::io::Error>(stderr_output)
    };

    let (write_result, stderr_result, read_result) =
        tokio::join!(write_future, stderr_future, read_future);

    write_result?;
    let stderr_output = stderr_result?;
    let output = read_result?;

    // Wait for the child process to finish and check the status
    let status = child.wait().await?;
    if !status.success() {
        bail!(
            "drawio-to-svg conversion failed with exit code: {}\nStderr: {}",
            status.code().unwrap_or(-1),
            stderr_output
        );
    }

    Ok(output)
}
