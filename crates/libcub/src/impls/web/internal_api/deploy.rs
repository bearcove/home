use crate::impls::{CubTenantImpl, cub_req::CubReqImpl, global_state};
use axum::extract::ws;
use camino::Utf8PathBuf;
use config_types::{TenantDomain, WebConfig};
use conflux::{InputPath, Pak, PathMappings};
use cub_types::{CubTenant, PathMetadata};
use libmomclient::MomTenantClient;
use librevision::{InputEvent, RevisionKind, RevisionSpec};
use libterm::FormatAnsiStyle;
use mom_types::ListMissingArgs;
use std::{collections::VecDeque, sync::Arc, time::Instant};
use tokio::io::AsyncBufReadExt;

pub(super) async fn serve(
    ws: axum::extract::WebSocketUpgrade,
    tr: CubReqImpl,
) -> impl axum::response::IntoResponse {
    let ts = tr.tenant.clone();
    let web = global_state().web;
    ws.on_upgrade(move |ws| handle_deploy_socket(ws, ts, web))
}

#[derive(Debug)]
enum DeployAction {
    StartDeploy(StartDeploy),
}

merde::derive! {
    impl (Serialize, Deserialize) for enum DeployAction
    externally_tagged {
        "startDeploy" => StartDeploy,
    }
}

#[derive(Debug)]
struct StartDeploy {
    ok: bool,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct StartDeploy {
        ok
    }
}

#[derive(Debug)]
enum DeployMessage {
    AssetProgress(AssetProgress),
    LogMessage(LogMessage),
    DeployComplete(DeployComplete),
}

#[derive(Debug)]
struct AssetCount {
    count: usize,
}

#[derive(Debug)]
struct AssetProgress {
    uploaded: usize,
    total: usize,
}

#[derive(Debug)]
pub struct LogMessage {
    pub(crate) level: Level,
    pub(crate) message: String,
}

#[allow(dead_code)]
impl LogMessage {
    pub fn info(message: impl std::fmt::Display) -> Self {
        Self {
            level: Level::Info,
            message: message.to_string(),
        }
    }

    pub fn warn(message: impl std::fmt::Display) -> Self {
        Self {
            level: Level::Warn,
            message: message.to_string(),
        }
    }

    pub fn error(message: impl std::fmt::Display) -> Self {
        Self {
            level: Level::Error,
            message: message.to_string(),
        }
    }

    #[allow(dead_code)]
    pub fn debug(message: impl std::fmt::Display) -> Self {
        Self {
            level: Level::Debug,
            message: message.to_string(),
        }
    }
}

#[derive(Debug)]
struct DeployComplete {
    complete: bool,
    domain: TenantDomain,
}

merde::derive! {
    impl (Serialize, Deserialize) for enum DeployMessage
    externally_tagged {
        "assetProgress" => AssetProgress,
        "logMessage" => LogMessage,
        "deployComplete" => DeployComplete,
    }
}

merde::derive! {
    impl (Serialize, Deserialize) for struct AssetCount {
        count
    }
}

merde::derive! {
    impl (Serialize, Deserialize) for struct AssetProgress {
        uploaded,
        total
    }
}

merde::derive! {
    impl (Serialize, Deserialize) for struct DeployComplete {
        complete, domain
    }
}

merde::derive! {
    impl (Serialize, Deserialize) for struct LogMessage {
        level, message
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

merde::derive! {
    impl (Serialize, Deserialize) for enum Level string_like {
        "debug" => Debug,
        "info" => Info,
        "warn" => Warn,
        "error" => Error,
    }
}

async fn json_to_socket(
    socket: &mut ws::WebSocket,
    payload: &(dyn merde::DynSerialize + Sync),
) -> eyre::Result<()> {
    let json_string = merde::json::to_string(payload)?;
    Ok(socket.send(ws::Message::Text(json_string)).await?)
}

async fn handle_deploy_socket(mut socket: ws::WebSocket, ts: Arc<CubTenantImpl>, web: WebConfig) {
    if let Err(e) = handle_deploy_socket_inner(&mut socket, ts, web).await {
        let error_message = DeployMessage::LogMessage(LogMessage {
            level: Level::Error,
            message: format!("Error: {e}"),
        });
        if let Err(send_err) = json_to_socket(&mut socket, &error_message).await {
            tracing::error!("Failed to send error message to websocket: {}", send_err);
        }
    }
}

/// Run vite build and incorporate the results into a new revision
async fn run_vite_build_and_update_revision(
    socket: &mut ws::WebSocket,
    ts: &Arc<CubTenantImpl>,
    mappings: PathMappings,
    web: WebConfig,
) -> eyre::Result<Arc<conflux::Revision>> {
    // Run svelte-check before proceeding with the build
    json_to_socket(
        socket,
        &DeployMessage::LogMessage(LogMessage {
            level: Level::Info,
            message: "Running svelte-check...".to_string(),
        }),
    )
    .await?;

    let svelte_check_output = tokio::process::Command::new("pnpm")
        .arg("dlx")
        .arg("svelte-check")
        .current_dir(&ts.ti().base_dir)
        .output()
        .await
        .map_err(|e| eyre::eyre!("Failed to run svelte-check: {}", e))?;

    if !svelte_check_output.status.success() {
        let error_output = String::from_utf8_lossy(&svelte_check_output.stderr);
        json_to_socket(
            socket,
            &DeployMessage::LogMessage(LogMessage {
                level: Level::Error,
                message: format!(
                    "svelte-check failed. Aborting deploy. Error: {error_output}"
                ),
            }),
        )
        .await?;
        return Err(eyre::eyre!("svelte-check failed. Aborting deploy."));
    }

    json_to_socket(
        socket,
        &DeployMessage::LogMessage(LogMessage {
            level: Level::Info,
            message: "svelte-check passed. Proceeding with build...".to_string(),
        }),
    )
    .await?;

    // Send message about building frontend assets
    json_to_socket(
        socket,
        &DeployMessage::LogMessage(LogMessage {
            level: Level::Info,
            message: "Building frontend assets with vite...".to_string(),
        }),
    )
    .await?;

    let start_time = std::time::Instant::now();
    let tenant_name = ts.tc().name.clone();
    let base_dir = ts.ti().base_dir.clone();

    let vite_build_dir = mappings
        .to_disk_path(&InputPath::from_static("/dist"))
        .expect("No mapping found for /dist path");

    tracing::info!(
        "[{tenant_name}] Using temporary build directory: {}",
        vite_build_dir
    );

    // Run npm build command with real-time output streaming
    let mut command = tokio::process::Command::new("npx");
    command
        .arg("vite")
        .arg("--config")
        .arg(ts.ti().vite_config_path())
        .arg("build")
        .arg("--mode")
        .arg("production")
        .arg("--outDir")
        .arg(&vite_build_dir)
        .env("NODE_ENV", "production")
        .env("FORCE_COLOR", "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .current_dir(&base_dir);

    // Get the term module to use for ANSI->HTML conversion
    let term_mod = libterm::load();

    // Start the command
    let mut child = command
        .spawn()
        .map_err(|e| eyre::eyre!("Failed to execute build step: {}", e))?;

    // Set up stdout streaming
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| eyre::eyre!("Failed to capture stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| eyre::eyre!("Failed to capture stderr"))?;

    // Create buffered readers
    let stdout_reader = tokio::io::BufReader::new(stdout);
    let stderr_reader = tokio::io::BufReader::new(stderr);

    // Create buffers
    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();

    // Process the output in a separate task so we can stream it back in real-time
    let (tx, mut rx) = tokio::sync::mpsc::channel::<(String, bool)>(100);

    // Spawn a task to handle stdout
    let tx_stdout = tx.clone();
    tokio::spawn(async move {
        let mut lines = tokio::io::BufReader::new(stdout_reader).lines();
        while let Some(line) = lines.next_line().await.unwrap() {
            if tx_stdout.send((line, false)).await.is_err() {
                break;
            }
        }
    });

    // Spawn a task to handle stderr
    let tx_stderr = tx.clone();
    tokio::spawn(async move {
        let mut lines = tokio::io::BufReader::new(stderr_reader).lines();
        while let Some(line) = lines.next_line().await.unwrap() {
            if tx_stderr.send((line, true)).await.is_err() {
                break;
            }
        }
    });

    // Drop the sender so the channel closes when both tasks complete
    drop(tx);

    // Process incoming chunks and send them to WebSocket
    while let Some((line, is_stderr)) = rx.recv().await {
        // Collect the output for success/failure determination
        if is_stderr {
            stderr_buf.extend_from_slice(line.as_bytes());
        } else {
            stdout_buf.extend_from_slice(line.as_bytes());
        }

        // Convert ANSI to HTML and send to WebSocket
        let cleaned_line = clean_build_output(&line, &vite_build_dir);
        if cleaned_line.trim().is_empty() {
            continue;
        }
        let html_output = term_mod.format_ansi(cleaned_line.trim(), FormatAnsiStyle::Html);

        // Send to client via WebSocket with appropriate level
        json_to_socket(
            socket,
            &DeployMessage::LogMessage(LogMessage {
                level: if is_stderr { Level::Warn } else { Level::Info },
                message: html_output.trim().to_string(),
            }),
        )
        .await?;
    }

    // Wait for the command to finish
    let status = child
        .wait()
        .await
        .map_err(|e| eyre::eyre!("Failed to wait for npm build process: {}", e))?;

    let build_elapsed = start_time.elapsed();

    if !status.success() {
        let error_msg = String::from_utf8_lossy(&stderr_buf);
        return Err(eyre::eyre!(
            "Vite build failed with status {}: {}",
            status,
            error_msg
        ));
    }

    json_to_socket(
        socket,
        &DeployMessage::LogMessage(LogMessage {
            level: Level::Info,
            message: format!(
                "[{tenant_name}] Frontend assets built successfully in {build_elapsed:?}"
            ),
        }),
    )
    .await?;

    // Now we need to create an incremental revision with the dist directory
    json_to_socket(
        socket,
        &DeployMessage::LogMessage(LogMessage {
            level: Level::Info,
            message: "Scanning built assets and creating new revision...".to_string(),
        }),
    )
    .await?;

    // Get current revision
    let rs = ts.revstate();
    let curr_rev = match &rs.rev {
        Some(indexed_rev) => indexed_rev.rev.clone(),
        None => return Err(eyre::eyre!("No current revision available")),
    };

    let ti = ts.ti().clone();

    // Create a synthetic "file created" event for the dist directory
    // This will make the revision process scan the entire directory
    let dist_metadata = tokio::fs::metadata(&vite_build_dir)
        .await
        .map_err(|e| eyre::eyre!("Failed to get metadata for dist directory: {}", e))?;
    let dist_input_path = InputPath::new("/dist".to_string());

    let event = InputEvent::Created {
        path: dist_input_path,
        metadata: PathMetadata::from(dist_metadata),
    };

    let mut events = VecDeque::new();
    events.push_back(event);

    // Create an incremental revision that incorporates the new dist files
    let rev_start = std::time::Instant::now();
    let kind = RevisionKind::Incremental {
        prev: curr_rev,
        events,
    };

    let spec = RevisionSpec { kind, mappings };

    // Create the new revision
    let revision_result = librevision::load().make_revision(ti, spec, web).await;
    let indexed_rev = match revision_result {
        Ok(rev) => rev,
        Err(e) => {
            let err_msg = format!("Failed to update revision with new assets: {e}");
            json_to_socket(
                socket,
                &DeployMessage::LogMessage(LogMessage {
                    level: Level::Error,
                    message: err_msg.clone(),
                }),
            )
            .await?;
            return Err(eyre::eyre!(err_msg));
        }
    };

    let rev_elapsed = rev_start.elapsed();
    json_to_socket(
        socket,
        &DeployMessage::LogMessage(LogMessage {
            level: Level::Info,
            message: format!("Revision updated with new assets in {rev_elapsed:?}"),
        }),
    )
    .await?;

    Ok(indexed_rev.rev)
}

async fn handle_deploy_socket_inner(
    socket: &mut ws::WebSocket,
    tenant: Arc<CubTenantImpl>,
    web: WebConfig,
) -> eyre::Result<()> {
    let tc = tenant.tc();

    // Create a temporary directory for the vite build
    let tenant_name = tc.name.clone();
    let vite_build_dir_guard = tempdir::TempDir::new(&format!("vite-build-{tenant_name}"))
        .map_err(|e| eyre::eyre!("Failed to create temporary build directory: {e}"))?;
    let vite_build_dir =
        Utf8PathBuf::from_path_buf(vite_build_dir_guard.path().to_path_buf()).unwrap();

    let mut mappings = PathMappings::from_ti(tenant.ti().as_ref());
    mappings.add(InputPath::from_static("/dist"), vite_build_dir.clone());

    // Run vite build and update the revision with the new assets
    let rev = run_vite_build_and_update_revision(socket, &tenant, mappings.clone(), web).await?;

    let gs = global_state();

    tracing::info!("[{tenant_name}] Making mom tenant client");
    let tcli: Arc<dyn MomTenantClient> =
        Arc::from(gs.mom_deploy_client.mom_tenant_client(tenant_name.clone()));

    tracing::info!("[{tenant_name}] Listing missing assets...");
    let missing_assets = tcli
        .objectstore_list_missing(&ListMissingArgs {
            objects_to_query: rev
                .pak
                .inputs
                .iter()
                .map(|(path, input)| (input.key(), path.clone()))
                .collect(),
            mark_these_as_uploaded: None,
        })
        .await?;

    let total_inputs = rev.pak.inputs.len();
    let missing_inputs = missing_assets.missing.len();
    let mut uploaded_inputs = total_inputs - missing_inputs;

    if missing_inputs > 0 {
        json_to_socket(
            socket,
            &DeployMessage::LogMessage(LogMessage {
                level: Level::Info,
                message: format!(
                    "Assets: {uploaded_inputs}/{total_inputs} already present, will upload {missing_inputs} new ones"
                ),
            }),
        )
        .await?;
    }

    json_to_socket(
        socket,
        &DeployMessage::AssetProgress(AssetProgress {
            uploaded: uploaded_inputs,
            total: total_inputs,
        }),
    )
    .await?;

    let (task_tx, task_rx) = flume::unbounded::<InputPath>();
    let (result_tx, result_rx) = flume::unbounded::<eyre::Result<()>>();

    const NUM_WORKERS: usize = 4;
    for _ in 0..NUM_WORKERS {
        let task_rx = task_rx.clone();
        let result_tx = result_tx.clone();
        let tcli = tcli.clone();
        let rev = rev.clone();
        let mappings = mappings.clone();

        tokio::spawn(async move {
            async fn handle_task(
                key: InputPath,
                curr_pak: &Pak<'static>,
                tcli: &dyn MomTenantClient,
                mappings: &PathMappings,
            ) -> eyre::Result<()> {
                let input = curr_pak
                    .inputs
                    .get(&key)
                    .ok_or_else(|| eyre::eyre!("Input not found in revision for key: {}", key))?;
                let disk_path = mappings.to_disk_path(&key)?;
                tracing::debug!("Reading input file from disk path: {:?}", disk_path);
                let before_read = Instant::now();
                let payload = match tokio::fs::read(&disk_path).await {
                    Ok(data) => data,
                    Err(e) => {
                        return Err(eyre::eyre!(
                            "Failed to read input file at {:?}: {}",
                            disk_path,
                            e
                        ));
                    }
                };
                let read_time = before_read.elapsed();

                let expected_hash = librevision::load().input_hash_from_contents(&payload);
                if expected_hash != input.hash {
                    return Err(eyre::eyre!(
                        "Hash mismatch for input {} (did things change on disk while we were deploying?): expected {:?}, got {:?}",
                        key,
                        input.hash,
                        expected_hash
                    ));
                }

                let before_upload = Instant::now();
                tcli.put_asset(&input.key(), payload.into()).await?;
                let upload_time = before_upload.elapsed();

                tracing::info!(
                    "Uploaded input {:?} (read: {:?}, upload: {:?})",
                    key,
                    read_time,
                    upload_time
                );

                Ok(())
            }

            while let Ok(key) = task_rx.recv_async().await {
                let result = handle_task(key.clone(), &rev.pak, tcli.as_ref(), &mappings).await;
                result_tx.send(result).unwrap();
            }
        });
    }
    drop(result_tx);

    // Send tasks to channel
    for (_object_store_key, input_path) in missing_assets.missing {
        task_tx.send(input_path).unwrap();
    }
    drop(task_tx);

    let mut num_errors = 0;

    while let Ok(res) = result_rx.recv_async().await {
        match res {
            Ok(_) => {
                uploaded_inputs += 1;
                json_to_socket(
                    socket,
                    &DeployMessage::AssetProgress(AssetProgress {
                        uploaded: uploaded_inputs,
                        total: total_inputs,
                    }),
                )
                .await?;
            }
            Err(e) => {
                num_errors += 1;
                json_to_socket(
                    socket,
                    &DeployMessage::LogMessage(LogMessage {
                        level: Level::Error,
                        message: format!("{e}"),
                    }),
                )
                .await?;
                break;
            }
        }
    }

    if num_errors > 0 {
        return Err(eyre::eyre!(
            "Encountered {} errors while uploading assets",
            num_errors
        ));
    }

    let mod_revision = librevision::load();
    let revpak = mod_revision.serialize_pak(&rev.pak);
    let revpak_size = revpak.len();
    let formatted_size = bytesize::to_string(revpak_size as u64, true);
    json_to_socket(
        socket,
        &DeployMessage::LogMessage(LogMessage {
            level: Level::Info,
            message: format!("Uploading revision package ({formatted_size})"),
        }),
    )
    .await?;

    let start_time = Instant::now();
    tcli.put_revpak(&rev.pak.id, revpak.into()).await?;
    let elapsed = start_time.elapsed();

    json_to_socket(
        socket,
        &DeployMessage::LogMessage(LogMessage {
            level: Level::Info,
            message: format!("Revision uploaded in {elapsed:?}"),
        }),
    )
    .await?;

    json_to_socket(
        socket,
        &DeployMessage::DeployComplete(DeployComplete {
            complete: true,
            domain: tenant.tc().name.clone(),
        }),
    )
    .await?;

    Ok(())
}

fn clean_build_output(line: &str, vite_build_dir: &Utf8PathBuf) -> String {
    // Replace the actual build directory path with "@/"
    let replaced = line.trim().replace(&vite_build_dir.to_string(), "@");

    // Find the position of "@" which replaced the build directory
    if let Some(pos) = replaced.find('@') {
        // Take everything after the "@" character, ensuring we handle any following slashes
        let result = &replaced[pos..];
        // If there's no slash after "@", add one
        if result == "@" {
            return "@/".to_string();
        } else if !result.starts_with("@/") {
            // If there's content after "@" but no slash, insert one
            return result.replace('@', "@/");
        }

        return result.to_string();
    }

    // If we couldn't find the replacement marker, clean up any leading dots and slashes
    let mut start_index = 0;
    let chars: Vec<char> = replaced.chars().collect();

    while start_index < chars.len() && (chars[start_index] == '.' || chars[start_index] == '/') {
        start_index += 1;
    }

    replaced[start_index..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;

    #[test]
    fn test_clean_build_output() {
        let vite_build_dir = Utf8PathBuf::from("/tmp/vite-build-dir");

        // Test case 1: Input with build directory path
        let input1 =
            "../../../../tmp/vite-build-dir/assets/IosevkaFTLNerdFont-Bold-subset-BpDlLZmV.woff2";
        let expected1 = "@/assets/IosevkaFTLNerdFont-Bold-subset-BpDlLZmV.woff2";
        assert_eq!(clean_build_output(input1, &vite_build_dir), expected1);

        // Test case 2: Input without build directory path
        let input2 = "Some other text without build directory";
        let expected2 = "Some other text without build directory";
        assert_eq!(clean_build_output(input2, &vite_build_dir), expected2);

        // Test case 3: Input with partial build directory path
        let input3 = "/tmp/vite-build-dir/src/main.js";
        let expected3 = "@/src/main.js";
        assert_eq!(clean_build_output(input3, &vite_build_dir), expected3);
    }
}
