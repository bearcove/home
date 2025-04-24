use std::{io::Write, sync::Arc};

use config_types::{TenantInfo, WebConfig, is_production};
use conflux::{Pak, PathMappings, RevisionError};
use cub_types::{CubRevisionState, IndexedRevision};
use load::load_pak;
use make::make_revision;
use merde::IntoStatic as _;
use tracing::warn;

use crate::{RevisionKind, RevisionSpec};

pub mod frontmatter;
pub mod load;
pub mod make;
pub mod watch;

pub fn revision_error_from_report(e: eyre::Report) -> RevisionError {
    let mut err: Vec<u8> = Default::default();
    use std::io::Write;
    writeln!(
        &mut err,
        "💀💀💀 Failed to make revision from scratch 💀💀💀"
    )
    .unwrap();
    print_error_to_writer(&e, &mut err);
    let err = String::from_utf8(err).unwrap();
    RevisionError(err)
}

pub fn print_error(e: &eyre::Report) {
    print_error_to_writer(e, &mut std::io::stderr());
}

pub fn print_error_to_writer(e: &eyre::Report, writer: &mut impl std::io::Write) {
    for (i, e) in e.chain().enumerate() {
        writeln!(writer, "{}. {}", i + 1, e).unwrap();
    }

    if let Some(bt) = liberrhandling::load().format_backtrace_to_terminal_colors(e) {
        writeln!(writer, "{bt}").unwrap();
    }
}

pub(crate) fn serialize_pak(pak: &Pak) -> Vec<u8> {
    // TODO: switch to something else
    merde::json::to_vec(pak).unwrap()
}

// Load the initial revision, either from disk or by creating a new one
pub(crate) async fn load_initial_revision(ti: Arc<TenantInfo>, web: WebConfig) -> CubRevisionState {
    // This is only ever supposed to happen in development, so we are only concerned with the content folder.
    // Frontend assets are served by a dev server.
    let mappings = PathMappings::from_ti(ti.as_ref());

    match load_revision_from_disk(ti.clone(), web).await {
        Ok(Some(irev)) => {
            // if we're in prod, that's enough
            if is_production() {
                return CubRevisionState {
                    rev: Some(irev),
                    err: None,
                };
            }

            // if we're in dev, look for changes and apply them
            tracing::debug!("Looking for changes, making a wake revision");
            match make_revision(
                ti.clone(),
                RevisionSpec {
                    kind: RevisionKind::Wake { prev: irev.clone() },
                    mappings: mappings.clone(),
                },
                web,
            )
            .await
            {
                Ok(new_irev) => {
                    return CubRevisionState {
                        rev: Some(new_irev),
                        err: None,
                    };
                }
                Err(e) => {
                    warn!("Failed to make wake revision");
                    print_error(&e);
                    return CubRevisionState {
                        // the initial rev gets served...
                        rev: Some(irev),
                        // _but_ we also show an error (and subsequent "incremental" revisions will be wake ones)
                        err: Some(conflux::RevisionError(e.to_string())),
                    };
                }
            }
        }
        res => {
            if let Err(e) = res {
                warn!("Failed to load active revision");
                print_error(&e);
                // fall through
            }
        }
    }

    // try to make a single revision from scratch
    match make_revision(
        ti.clone(),
        RevisionSpec {
            kind: RevisionKind::FromScratch,
            mappings: mappings.clone(),
        },
        web,
    )
    .await
    {
        Ok(indexed_rev) => {
            tracing::debug!("Successfully created a new revision from scratch");
            if let Err(err) = save_pak_to_disk_as_active(&indexed_rev.rev.pak, ti.as_ref()).await {
                warn!("Failed to save pak to disk: {err}");
            }
            CubRevisionState {
                rev: Some(indexed_rev),
                err: None,
            }
        }
        Err(e) => {
            let mut err: Vec<u8> = Default::default();
            use std::io::Write;
            writeln!(
                &mut err,
                "💀💀💀 Failed to make revision from scratch 💀💀💀"
            )
            .unwrap();
            print_error_to_writer(&e, &mut err);
            let err = String::from_utf8(err).unwrap();

            CubRevisionState {
                rev: None,
                err: Some(conflux::RevisionError(err)),
            }
        }
    }
}

/// Reads the revision pak marked as "active" from disk, and load it
pub(crate) async fn load_revision_from_disk(
    ti: Arc<TenantInfo>,
    web: WebConfig,
) -> eyre::Result<Option<IndexedRevision>> {
    let internal_dir = ti.internal_dir();
    let revisions_dir = internal_dir.join("revisions");
    let mappings = PathMappings::from_ti(ti.as_ref());

    // the active revision ID is written in a file named `{internal_dir}/revisions/00000_active`
    let rev_id = match tokio::fs::read_to_string(revisions_dir.join("active")).await {
        Ok(id) => id,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    // the active revision pack is stored in a file named `{internal_dir}/revisions/rev_id.revpak`
    let rev_pack_path = revisions_dir.join(format!("{rev_id}.revpak"));

    // read it as a string
    let source = tokio::fs::read_to_string(&rev_pack_path).await?;

    // deserialize it
    let pak: Pak = merde::json::from_str_owned(&source).map_err(|e| e.into_static())?;
    let rev = load_pak(pak, ti, None, mappings, web).await?;
    Ok(Some(rev))
}

/// Save the revision pack to disk if needed, and make it the
/// active one.
pub async fn save_pak_to_disk_as_active(pak: &Pak, ti: &TenantInfo) -> eyre::Result<()> {
    let internal_dir = ti.internal_dir();
    let revisions_dir = internal_dir.join("revisions");

    let pak_file_path = revisions_dir.join(format!("{}.revpak", pak.id));

    if !pak_file_path.exists() {
        // Create the revisions directory if it doesn't exist
        tokio::fs::create_dir_all(&revisions_dir).await?;

        // Serialize the revision to JSON
        let serialized = serialize_pak(pak);

        // Write the serialized revision to a file atomically
        let af = atomicwrites::AtomicFile::new(&pak_file_path, atomicwrites::AllowOverwrite);
        af.write(|f| f.write_all(&serialized))?;
    }

    // Update the active revision file atomically
    let active_file_path = revisions_dir.join("active");
    let af = atomicwrites::AtomicFile::new(&active_file_path, atomicwrites::AllowOverwrite);
    af.write(|f| f.write_all(pak.id.to_string().as_bytes()))?;

    // List all files in the revisions directory that start with 'rev_'
    let mut rev_files = Vec::new();
    let mut read_dir = tokio::fs::read_dir(&revisions_dir).await?;
    while let Some(entry) = read_dir.next_entry().await? {
        let file_name = match entry.file_name().into_string() {
            Ok(name) => name,
            Err(_) => continue,
        };

        if file_name.starts_with("rev_") {
            if let Ok(metadata) = entry.metadata().await {
                if let Ok(modified) = metadata.modified() {
                    rev_files.push((entry.path(), modified));
                }
            }
        }
    }

    // Sort the files by modification time, most recent first
    rev_files.sort_by(|a, b| b.1.cmp(&a.1));

    // Keep only the 5 most recent files
    let files_to_keep: Vec<_> = rev_files
        .into_iter()
        .take(5)
        .map(|(path, _)| path)
        .collect();

    // Remove all other rev_ files
    let mut read_dir = tokio::fs::read_dir(&revisions_dir).await?;
    while let Some(entry) = read_dir.next_entry().await? {
        let path = entry.path();
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("rev_"))
            && !files_to_keep.contains(&path)
        {
            if let Err(e) = tokio::fs::remove_file(&path).await {
                if e.kind() != std::io::ErrorKind::NotFound {
                    warn!("Failed to remove {}: {e}", path.display());
                }
            }
        }
    }

    Ok(())
}
