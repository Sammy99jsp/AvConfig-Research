use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use color_eyre::Result;
use notify::{
    event::{AccessKind, AccessMode},
    Event, EventKind, INotifyWatcher, RecursiveMode, Watcher,
};
use tokio::{sync::watch, task::JoinHandle};

use crate::config3::HAS_DESERIALIZED;

pub struct FileHander {
    path: PathBuf,
    watcher: INotifyWatcher,
    saver: JoinHandle<()>,
    rx: watch::Receiver<String>,
    tx: watch::Sender<String>,
}

static HAS_WRITTEN: AtomicBool = AtomicBool::new(false);

impl FileHander {
    fn file_watcher(path: &Path, tx: watch::Sender<String>) -> Result<INotifyWatcher> {
        let path_inner = path.to_owned();
        // Watch file when read..
        let mut watcher = notify::recommended_watcher(move |res| {
            let ev = match res {
                Ok(ev) => ev,
                Err(err) => {
                    eprintln!("{err:?}");
                    return;
                }
            };

            // If this was written to.
            if matches!(
                ev,
                Event {
                    kind: EventKind::Access(AccessKind::Close(AccessMode::Write)),
                    ..
                }
            ) {
                if HAS_WRITTEN.load(Ordering::SeqCst) {
                    HAS_WRITTEN.store(false, Ordering::SeqCst);
                    return;
                }

                if let Ok(contents) = std::fs::read_to_string(&path_inner) {
                    tx.send_if_modified(|s| {
                        let modified = s != &contents;
                        if modified {
                            *s = contents;
                        }

                        modified
                    });
                }
            }
        })?;

        watcher.watch(path, RecursiveMode::NonRecursive)?;

        Ok(watcher)
    }

    fn file_saver(path: &Path, mut rx: watch::Receiver<String>) -> JoinHandle<()> {
        let path = path.to_owned();
        tokio::spawn(async move {
            while let Ok(()) = rx.changed().await {
                let contents = rx.borrow();

                if HAS_WRITTEN.load(Ordering::SeqCst) && HAS_DESERIALIZED.load(Ordering::SeqCst) {
                    continue;
                }

                println!("Written to file!");
                HAS_WRITTEN.store(true, Ordering::SeqCst);
                std::fs::write(&path, contents.as_bytes()).unwrap();
            }
        })
    }

    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let (tx, rx) = watch::channel(std::fs::read_to_string(&path)?);
        let watcher = Self::file_watcher(&path, tx.clone())?;
        let saver = Self::file_saver(&path, rx.clone());
        Ok(Self {
            path,
            watcher,
            saver,
            rx,
            tx,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn stop(mut self) -> Result<()> {
        self.saver.abort();
        self.watcher.unwatch(&self.path)?;

        Ok(())
    }

    pub(super) fn tx(&self) -> watch::Sender<String> {
        self.tx.clone()
    }

    pub(super) fn rx(&self) -> watch::Receiver<String> {
        self.rx.clone()
    }
}
