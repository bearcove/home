use autotrait::autotrait;
use camino::Utf8PathBuf;
use std::{fs::FileType, path::Path};

use ignore::overrides::OverrideBuilder;
use notify::{Watcher as _, recommended_watcher};

#[derive(Default)]
struct ModImpl;

pub fn load() -> &'static dyn Mod {
    static MOD: ModImpl = ModImpl;
    &MOD
}

#[derive(Debug)]
pub struct WatcherEvent {
    pub kind: WatcherEventKind,
    pub paths: Vec<Utf8PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WatcherEventKind {
    Create,
    Remove,
    Modify,
}

pub type WalkDirItem = eyre::Result<Box<dyn DirEntry>>;
pub type WalkDirIter = Box<dyn Iterator<Item = WalkDirItem>>;

fn from_notify_kind(kind: notify::EventKind) -> Option<WatcherEventKind> {
    match kind {
        notify::EventKind::Create(_) => Some(WatcherEventKind::Create),
        notify::EventKind::Modify(_) => Some(WatcherEventKind::Modify),
        notify::EventKind::Remove(_) => Some(WatcherEventKind::Remove),
        _ => None,
    }
}

#[autotrait]
impl Mod for ModImpl {
    fn walkdir(&self, path: &Path) -> eyre::Result<WalkDirIter> {
        let path = Path::new(path);
        let overrides = OverrideBuilder::new(".").build()?;
        let walker = ignore::WalkBuilder::new(path).overrides(overrides).build();
        Ok(Box::new(walker.map(|entry| {
            entry
                .map(|e| Box::new(DirEntryWrapper(e)) as Box<dyn DirEntry>)
                .map_err(|e| eyre::eyre!(e))
        })))
    }

    fn make_watcher(
        &self,
        on_event: Box<dyn Fn(eyre::Result<WatcherEvent>) + Send + Sync>,
    ) -> Box<dyn Watcher> {
        Box::new(WatcherWrapper(
            recommended_watcher(
                move |ev_res: Result<notify::Event, notify::Error>| match ev_res {
                    Ok(ev) => {
                        if let Some(kind) = from_notify_kind(ev.kind) {
                            let ev = WatcherEvent {
                                paths: ev
                                    .paths
                                    .into_iter()
                                    .map(|p| p.try_into().expect("home only supports utf-8 paths"))
                                    .collect(),
                                kind,
                            };
                            on_event(Ok(ev))
                        }
                    }
                    Err(e) => on_event(Err(eyre::eyre!(e))),
                },
            )
            .unwrap(),
        ))
    }
}

struct DirEntryWrapper(ignore::DirEntry);

#[autotrait]
impl DirEntry for DirEntryWrapper {
    fn file_type(&self) -> Option<FileType> {
        self.0.file_type()
    }

    fn path(&self) -> &Path {
        self.0.path()
    }

    fn metadata(&self) -> eyre::Result<std::fs::Metadata> {
        self.0.metadata().map_err(|e| eyre::eyre!(e))
    }
}

struct WatcherWrapper(notify::RecommendedWatcher);

#[autotrait]
impl Watcher for WatcherWrapper {
    fn watch(&mut self, path: &Path) -> eyre::Result<()> {
        self.0.watch(path, notify::RecursiveMode::Recursive)?;
        Ok(())
    }
}
