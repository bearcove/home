use crate::{
    InputEvent, RevisionKind, RevisionSpec,
    impls::{
        make::{is_path_ignored, make_revision},
        revision_error_from_report,
    },
};
use ::libfs::{WatcherEvent, WatcherEventKind};
use config_types::WebConfig;
use conflux::{PathMappings, ROOT_INPUT_PATHS};
use cub_types::{CubTenant, PathMetadata};
use eyre::Result;
use itertools::Itertools;
use std::{collections::VecDeque, sync::Arc, time::Duration};
use tokio::sync::mpsc;
use tracing::{info, warn};

/// Start watching for file changes, trigger a new revision
/// when they do change
pub async fn start_watching(tenant: Arc<dyn CubTenant>, web: WebConfig) -> Result<()> {
    // We don't need to care about dist when watching because we're not actually watching dist for changes. We're only watching content for changes.
    let mappings = PathMappings::from_ti(tenant.ti());

    let prefix = format!("\x1b[35m{}\x1b[0m", tenant.tc().name.clone());
    let (watch_tx, mut watch_rx) = mpsc::channel::<WatcherEvent>(128);

    let tenant_for_spawn = tenant.clone();
    tokio::spawn(async move {
        let tenant = tenant_for_spawn;
        'recv: loop {
            let mut events = vec![];
            match watch_rx.recv_many(&mut events, 100).await {
                0 => break 'recv,
                n => {
                    tracing::info!("[{prefix}] Received {n} fs events",);
                }
            };
            loop {
                // try to receive some more until there's a quiet
                match tokio::time::timeout(
                    Duration::from_millis(16),
                    watch_rx.recv_many(&mut events, 100),
                )
                .await
                {
                    Ok(0) => {
                        break 'recv;
                    }
                    Ok(n) => {
                        tracing::info!("Received additional {n} events");
                    }
                    Err(_) => {
                        break;
                    }
                }
            }

            let rs = tenant.revstate();

            let kind = if let Some(prev) = &rs.rev {
                if rs.err.is_some() {
                    tracing::info!(
                        "[{prefix}] Have previous revision but also have error, making a wake revision"
                    );
                    RevisionKind::Wake { prev: prev.clone() }
                } else {
                    tracing::info!("[{prefix}] Making incremental revision");
                    // TODO: Do not unwrap, just mark a revision error instead.
                    let events = convert_watcher_events_to_input_events(events, &mappings)
                        .await
                        .unwrap();
                    RevisionKind::Incremental {
                        prev: prev.rev.clone(),
                        events,
                    }
                }
            } else {
                tracing::info!("[{prefix}] No previous good revision, making from scratch");
                RevisionKind::FromScratch
            };
            let spec = RevisionSpec {
                kind,
                mappings: mappings.clone(),
            };
            let res = make_revision(tenant.ti().clone(), spec, web).await;
            let indexed_rev = match res {
                Ok(rev) => rev,
                Err(e) => {
                    let e = revision_error_from_report(e);
                    tenant.write_to_revstate(&mut |state| {
                        state.err = Some(e.clone());
                    });
                    tenant.broadcast_error(e);

                    continue 'recv;
                }
            };
            tenant.switch_to(indexed_rev);
        }
        info!("[{prefix}] Watcher thread: disconnected!");
    });

    let prefix = format!("\x1b[35m{}\x1b[0m", tenant.tc().name);
    let fs = ::libfs::load();
    let (event_tx, mut event_rx) = mpsc::channel::<eyre::Result<WatcherEvent>>(128);

    // Spawn background task to handle async path filtering
    tokio::spawn(async move {
        while let Some(res) = event_rx.recv().await {
            match res {
                Ok(event) => {
                    let mut filtered_paths = Vec::new();
                    for path in event.paths {
                        if !is_path_ignored(&path).await {
                            filtered_paths.push(path);
                        }
                    }
                    if !filtered_paths.is_empty() {
                        watch_tx
                            .send(WatcherEvent {
                                kind: event.kind,
                                paths: filtered_paths,
                            })
                            .await
                            .unwrap();
                    }
                }
                Err(e) => warn!("Watch error: {:?}", e),
            }
        }
    });

    let event_tx_clone = event_tx.clone();
    let mut watcher = fs.make_watcher(Box::new(move |res: eyre::Result<WatcherEvent>| {
        event_tx_clone.blocking_send(res).unwrap();
    }));

    let mappings = PathMappings::from_ti(tenant.ti());
    for path in ROOT_INPUT_PATHS {
        let disk_path = mappings.to_disk_path(path).unwrap();
        if !disk_path.exists() {
            info!("[{prefix}] Creating {disk_path}");
            tokio::fs::create_dir_all(&disk_path).await.unwrap();
        }

        info!("[{prefix}] Watching \x1b[33m{disk_path}\x1b[0m");
        watcher.watch(disk_path.as_std_path())?;
    }
    std::mem::forget(watcher);

    Ok(())
}

async fn convert_watcher_events_to_input_events(
    events: Vec<WatcherEvent>,
    mappings: &PathMappings,
) -> eyre::Result<VecDeque<InputEvent>> {
    let mut input_events = Vec::new();

    for ev in events {
        let kind = ev.kind;
        for path in ev.paths {
            let input_path = mappings.to_input_path(&path)?;
            let input_event = match kind {
                WatcherEventKind::Create => match tokio::fs::metadata(&path).await {
                    Ok(metadata) => InputEvent::Created {
                        path: input_path,
                        metadata: PathMetadata::from(metadata),
                    },
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        InputEvent::Removed(input_path)
                    }
                    Err(e) => panic!("Failed to get metadata for {}: {}", path, e),
                },
                WatcherEventKind::Modify => match tokio::fs::metadata(&path).await {
                    Ok(metadata) => InputEvent::Modified {
                        path: input_path,
                        metadata: PathMetadata::from(metadata),
                    },
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        InputEvent::Removed(input_path)
                    }
                    Err(e) => panic!("Failed to get metadata for {}: {}", path, e),
                },
                WatcherEventKind::Remove => InputEvent::Removed(input_path),
            };
            input_events.push(input_event);
        }
    }

    Ok(input_events.into_iter().unique().collect())
}
