//! Hot-reload watcher: forwards filesystem events to exact-path invalidation.

use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use crate::{Vfs, VfsError};

/// Handle for the background watcher owned by a [`Vfs`].
pub struct VfsWatcher {
    watcher: Option<RecommendedWatcher>,
    handle: Option<JoinHandle<()>>,
}

impl VfsWatcher {
    /// Stops the watcher and joins its background thread.
    pub fn stop(mut self) {
        drop(self.watcher.take());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Vfs {
    /// Starts watching the base directory and every mod root.
    ///
    /// Each event goes through the filesystem-grounded invalidation primitive;
    /// content is re-read on the next [`read`](Vfs::read).
    pub fn watch(self: Arc<Self>) -> Result<VfsWatcher, VfsError> {
        let (sender, receiver) = mpsc::channel();
        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                if let Ok(event) = result {
                    let _ = sender.send(event);
                }
            })
            .map_err(map_watch_error)?;

        watcher
            .watch(&self.base_dir, RecursiveMode::Recursive)
            .map_err(map_watch_error)?;
        for layer in &self.mods {
            let (path, recursive) = layer.watch_target();
            watcher
                .watch(
                    path,
                    if recursive {
                        RecursiveMode::Recursive
                    } else {
                        RecursiveMode::NonRecursive
                    },
                )
                .map_err(map_watch_error)?;
        }

        let vfs = Arc::clone(&self);
        let handle = std::thread::Builder::new()
            .name("oxide-vfs-watch".to_owned())
            .spawn(move || {
                for event in receiver {
                    vfs.apply_event(&event);
                }
            })
            .map_err(VfsError::Io)?;

        Ok(VfsWatcher {
            watcher: Some(watcher),
            handle: Some(handle),
        })
    }

    /// Applies one event exactly as the background thread does.
    pub(crate) fn apply_event(&self, event: &notify::Event) {
        for path in &event.paths {
            self.invalidate(path);
        }
    }
}

fn map_watch_error(err: notify::Error) -> VfsError {
    let message = err.to_string();
    match err.kind {
        notify::ErrorKind::Io(io) => VfsError::Io(io),
        _ => VfsError::Io(std::io::Error::other(message)),
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use notify::event::{DataChange, EventKind, ModifyKind};

    use super::*;
    use crate::ResKind;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("oxide-vfs-watch-{}-{label}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn synthetic_event_does_not_trust_event_path_casing() {
        let base = TempDir::new("synthetic-casing");
        let file = base.path().join("data/foo.json");
        std::fs::create_dir_all(file.parent().expect("file parent")).expect("create data dir");
        std::fs::write(&file, b"disk").expect("write file");

        let vfs = Arc::new(Vfs::new(base.path().to_path_buf()));
        let source = vfs.resolve("data/foo.json").expect("disk casing resolves");
        assert_eq!(vfs.read(&source, 16).expect("read disk"), b"disk");

        vfs.apply_event(&modify_event(&base.path().join("DATA/FOO.JSON")));
        assert!(
            vfs.resolve("DATA/FOO.JSON").is_none(),
            "event casing must not enter the index"
        );
        assert!(vfs.resolve("data/FOO.JSON").is_none());

        vfs.apply_event(&modify_event(&base.path().join("data/FOO.JSON")));
        assert!(vfs.resolve("data/FOO.JSON").is_none());
        let source = vfs
            .resolve("data/foo.json")
            .expect("on-disk casing still resolves");
        assert_eq!(vfs.read(&source, 16).expect("read disk"), b"disk");
    }

    #[test]
    fn synthetic_event_invalidates_exactly_the_changed_path() {
        let base = TempDir::new("synthetic");
        let chart = base.path().join("data/x/x-chart.json");
        std::fs::create_dir_all(chart.parent().expect("chart parent")).expect("create data dir");
        std::fs::write(&chart, b"A").expect("write A");

        let vfs = Arc::new(Vfs::new(base.path().to_path_buf()));
        let source = vfs
            .resolve_kind(ResKind::Json, "x/x-chart")
            .expect("chart resolves");
        assert_eq!(vfs.read(&source, 16).expect("read A"), b"A");

        std::fs::write(&chart, b"B").expect("write B");
        vfs.apply_event(&modify_event(&chart));
        let source = vfs
            .resolve_kind(ResKind::Json, "x/x-chart")
            .expect("chart still resolves");
        assert_eq!(vfs.read(&source, 16).expect("read B"), b"B");

        std::fs::remove_file(&chart).expect("delete chart");
        vfs.apply_event(&modify_event(&chart));
        assert!(
            vfs.resolve_kind(ResKind::Json, "x/x-chart").is_none(),
            "deleted chart must not resolve"
        );
    }

    fn modify_event(path: &Path) -> notify::Event {
        notify::Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
            .add_path(path.to_path_buf())
    }
}
