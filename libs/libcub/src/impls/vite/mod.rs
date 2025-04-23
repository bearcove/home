use std::sync::Arc;

use config::{Environment, TenantDomain, TenantInfo, WebConfig};
use http::Uri;
use regex;
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::info;

/// Generates a `.home/vite.config.js` and `tsconfig.json`
async fn generate_vite_configs(ti: &TenantInfo, web: WebConfig) -> eyre::Result<()> {
    let tenant_name = ti.tc.name.clone();
    let vite_config_path = ti.vite_config_path();

    // todo: generate a `vite.config.js` file in the internal dir, based
    let template = include_str!("./vite.config.js");

    // cf. https://vite.dev/config/shared-options.html — the origin part isn't used in
    // development
    let prod_web = WebConfig {
        port: 0,
        env: Environment::Production,
    };
    let template = template.replace("%BASE%", ti.tc.cdn_base_url(prod_web).as_str());

    // cf. https://vite.dev/config/server-options.html#server-origin — only used in dev,
    // because we use vite serve for HMR (and proxy it)
    let template = template.replace("%SERVER_ORIGIN%", ti.tc.cdn_base_url(web).as_str());
    // because we use vite serve for HMR (and proxy it)
    let template = template.replace("%SERVER_CORS_ORIGIN%", ti.tc.web_base_url(web).as_str());

    // Replace HMR configuration values
    let uri = Uri::try_from(ti.tc.cdn_base_url(web).as_str()).unwrap();
    let template = template.replace("%SERVER_HMR_HOST%", uri.host().unwrap());
    let template = template.replace(
        "%SERVER_HMR_CLIENT_PORT%",
        &uri.port_u16().unwrap().to_string(),
    );

    // First, generate the vite config file
    tokio::fs::write(&vite_config_path, template)
        .await
        .map_err(|e| eyre::eyre!("[{tenant_name}] Failed to write vite config: {e}"))?;

    tracing::debug!("[{tenant_name}] Installing required vite dev dependencies");

    // Helper function to run pnpm install commands
    async fn run_pnpm_install(
        ti: &TenantInfo,
        deps: &[&str],
        dev_flag: bool,
        dep_type: &str,
    ) -> eyre::Result<()> {
        let tenant_name = &ti.tc.name;

        let mut cmd = tokio::process::Command::new("pnpm");
        cmd.arg("install");

        if dev_flag {
            cmd.arg("-D");
        }

        cmd.args(deps).current_dir(&ti.base_dir);

        // Execute the command
        let output = cmd.output().await.map_err(|e| {
            eyre::eyre!("[{tenant_name}] Failed to install {dep_type} dependencies: {e}")
        })?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(eyre::eyre!(
                "[{tenant_name}] Failed to install {dep_type} dependencies: {error_msg}"
            ));
        } else {
            tracing::debug!("[{tenant_name}] Successfully installed {dep_type} dependencies");
        }

        Ok(())
    }

    let required_dev_deps = [
        "vite",
        "@sveltejs/vite-plugin-svelte",
        "vite-plugin-wasm",
        "vite-plugin-top-level-await",
        "rollup-plugin-visualizer",
        "sass-embedded",
        "@tsconfig/svelte",
    ];
    // Install dev dependencies
    run_pnpm_install(ti, &required_dev_deps, true, "development").await?;

    let required_normal_deps = ["svelte", "@bearcove/home-base"];
    // Install normal dependencies
    run_pnpm_install(ti, &required_normal_deps, false, "normal").await?;

    // Also generate tsconfig
    let tsconfig_config_path = ti.base_dir.join("tsconfig.json");
    let tsconfig_template = include_str!("./tsconfig.json");
    tokio::fs::write(&tsconfig_config_path, tsconfig_template)
        .await
        .map_err(|e| eyre::eyre!("[{tenant_name}] Failed to write tsconfig.json: {e}"))?;

    Ok(())
}

pub(crate) async fn start_vite(ti: Arc<TenantInfo>, web: WebConfig) -> eyre::Result<u16> {
    generate_vite_configs(ti.as_ref(), web).await?;

    let tenant_name = ti.tc.name.clone();

    tracing::debug!("[{tenant_name}] Installing pnpm dependencies");
    let output = tokio::process::Command::new("pnpm")
        .arg("i")
        .current_dir(&ti.base_dir)
        .output()
        .await
        .map_err(|e| eyre::eyre!("Failed to execute npm i: {}", e))?;

    if !output.status.success() {
        tracing::warn!(
            "pnpm install failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// Run a vite server in the background, which we can proxy to
    async fn run_vite(ti: Arc<TenantInfo>, web: WebConfig) -> eyre::Result<u16> {
        let tenant_name = ti.tc.name.clone();
        let base_dir = &ti.base_dir;

        let pid_file_path = ti.internal_dir().join("vite.pid");
        let _ = tokio::fs::create_dir_all(pid_file_path.parent().unwrap()).await;
        if pid_file_path.exists() {
            if let Ok(pid_str) = tokio::fs::read_to_string(&pid_file_path).await {
                if let Ok(pid) = pid_str.trim().parse::<i32>() {
                    tracing::debug!("[{tenant_name}] Killing previous vite process with PID {pid}");
                    if let Err(e) = nix::sys::signal::kill(
                        nix::unistd::Pid::from_raw(pid),
                        nix::sys::signal::Signal::SIGKILL,
                    ) {
                        // Don't log ESRCH errors (No such process)
                        if !matches!(e, nix::Error::ESRCH) {
                            tracing::warn!(
                                "[{tenant_name}] Failed to kill previous vite process: {e}"
                            );
                        }
                    }
                }
            }
            let _ = tokio::fs::remove_file(&pid_file_path).await;
        }

        tracing::debug!("[{tenant_name}] Starting vite");
        let base_dir_clone = base_dir.clone();
        // this is a oneshot in spirit
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);

        let ti_spawn = ti.clone();
        tokio::spawn(async move {
            let mut command = tokio::process::Command::new("pnpx");
            command
                .arg("vite")
                .arg("--config")
                .arg(ti_spawn.vite_config_path())
                .arg("--mode")
                .arg("development")
                .env("NODE_ENV", "development")
                .env("FORCE_COLOR", "1")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .current_dir(&base_dir_clone);

            // Only Linux gets the nice "I'm taking you with me" feature for now.
            #[cfg(target_os = "linux")]
            unsafe {
                command.pre_exec(|| {
                    libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                    Ok(())
                });
            }

            let mut child = command
                .spawn()
                .unwrap_or_else(|_| panic!("Failed to start vite"));
            let pid = child.id().expect("Failed to get child PID");
            tokio::fs::write(&pid_file_path, pid.to_string())
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!("[{tenant_name}] Failed to write PID file for vite: {e}");
                });

            let stdout = child.stdout.take().expect("Failed to capture stdout");
            let stderr = child.stderr.take().expect("Failed to capture stderr");

            tokio::spawn(relay_output(
                stdout,
                tenant_name.clone(),
                tx.clone(),
                ti_spawn.clone(),
                web,
            ));
            tokio::spawn(relay_output(
                stderr,
                tenant_name.clone(),
                tx,
                ti_spawn.clone(),
                web,
            ));

            async fn relay_output<R: tokio::io::AsyncRead + Unpin>(
                reader: R,
                tenant_name: TenantDomain,
                tx: tokio::sync::mpsc::Sender<u16>,
                ti: Arc<TenantInfo>,
                web: WebConfig,
            ) {
                let mut lines = BufReader::new(reader).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    // Check if the line contains localhost URL and extract the port
                    if let Some(port) = extract_vite_port(&line) {
                        info!("[{tenant_name}] Vite server running on port {port}");
                        // Send the port through the channel
                        let _ = tx.send(port).await;

                        // Print a user-friendly message with the URL to visit
                        let base_url = ti.tc.web_base_url(web);
                        info!(
                            "[{tenant_name}] Visit your site at: 🌐 \x1b[32m\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\\x1b[0m",
                            base_url, base_url
                        );

                        // Skip printing this line to avoid confusing the user
                        continue;
                    }

                    // Skip empty lines or lines with only whitespace
                    if !line.trim().is_empty() {
                        eprintln!("{line}");
                    }
                }
            }

            match child.wait().await {
                Ok(status) => {
                    if !status.success() {
                        tracing::error!(
                            "[{tenant_name}] Frontend vite process exited with status: {status}"
                        );
                    }
                }
                Err(e) => {
                    tracing::error!("[{tenant_name}] Error waiting for frontend vite process: {e}")
                }
            }
        });

        // Wait for the port to be received with a timeout
        match tokio::time::timeout(std::time::Duration::from_secs(30), rx.recv()).await {
            Ok(Some(port)) => Ok(port),
            Ok(None) => Err(eyre::eyre!(
                "Vite server failed to start: channel closed unexpectedly"
            )),
            Err(_) => Err(eyre::eyre!("Vite server failed to start: timeout occurred")),
        }
    }

    // The 'serve' command runs a development HTTP server that serves the compiled files.
    // In development, the server will provide URLs to the vite dev server,
    // allowing for hot reloading and other development features.
    run_vite(ti, web).await
}

/// Extract the port from a Vite server output line that contains "127.0.0.1:".
pub fn extract_vite_port(line: &str) -> Option<u16> {
    // For reference (with ANSI escapes, we asked for colors)
    /*
    > vite serve
      ^[[32m�M-^^M-^\^[[39m  ^[[1mLocal^[[22m:   ^[[36mhttp://127.0.0.1:^[[1m5173^[[22m/^[[39m
    */

    // Strip ANSI escape codes
    let stripped_line = strip_ansi_escapes::strip(line);
    let line =
        std::str::from_utf8(&stripped_line).expect("since when does vite output non-utf8 output?");

    // Using regex to extract the port number after "http://127.0.0.1"
    // Match "http://127.0.0.1:" followed by any characters until we find digits
    let regex = regex::Regex::new(r"http://127\.0\.0\.1:.*?(\d+)").unwrap();

    regex
        .captures(line)
        .and_then(|caps| caps.get(1).and_then(|m| m.as_str().parse::<u16>().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_vite_port() {
        // Test with standard Vite output
        let line = "  \x1b[32m➜\x1b[39m  \x1b[1mLocal\x1b[22m:   \x1b[36mhttp://127.0.0.1:\x1b[1m5173\x1b[22m/\x1b[39m";
        assert_eq!(extract_vite_port(line), Some(5173));

        // Test with different port
        let line = "Local:   http://127.0.0.1:3000/";
        assert_eq!(extract_vite_port(line), Some(3000));

        // Test with no port
        let line = "Error: Failed to start server";
        assert_eq!(extract_vite_port(line), None);

        // Test with invalid port
        let line = "Local:   http://127.0.0.1:abc/";
        assert_eq!(extract_vite_port(line), None);
    }
}
