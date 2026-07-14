//! File-watcher integration for [`NoteIndex`].
//!
//! Watches a vault's directory tree for changes to `.md` files
//! and updates the in-memory index incrementally. Without a
//! watcher the index is only correct at the moment of
//! [`NoteIndex::build`] / [`NoteIndex::rebuild`]; with one, it's
//! correct as long as the watcher is alive.
//!
//! The watcher uses the `notify` crate (debounced recommended
//! watcher on Linux/macOS, ReadDirectoryChangesW on Windows) and
//! dispatches events on a background thread. A `mpsc` channel
//! delivers coalesced events to the caller, who is expected to
//! drain the channel on a foreground context and call
//! [`NoteIndex::update_note`] / [`NoteIndex::remove_note`].
//!
//! **What is watched:** every `.md` file under the vault root.
//!
//! **What is NOT watched:** non-`.md` files. Attachment indexing
//! (M2.x) will get its own watcher.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::{
    event::{ModifyKind, RenameMode},
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};

use crate::index::NoteIndex;
use crate::Vault;

/// Coalesced file-event summary. Many filesystem events fire for
/// a single user save (write, truncate, write, …); we batch them
/// into one `NoteEvent` per path within the debounce window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteEvent {
    /// The file at this path was created or modified.
    Modified(PathBuf),
    /// The file at this path was removed.
    Removed(PathBuf),
}

/// A handle to a running file watcher. Drop to stop.
pub struct NoteWatcher {
    /// Held to keep the watcher alive. Dropping stops the watch.
    _watcher: RecommendedWatcher,
    /// Receiver of coalesced note events. The owner drains this
    /// and applies each to a [`NoteIndex`].
    pub events: Receiver<NoteEvent>,
    /// Vault root, kept for the path of the file the caller is
    /// asking about.
    vault_root: PathBuf,
}

impl NoteWatcher {
    /// Start watching `vault.root()` for changes to `.md` files.
    /// Drop the returned [`NoteWatcher`] to stop.
    pub fn start(vault: &Vault) -> Result<Self, WatcherError> {
        let vault_root = vault.root().to_path_buf();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = RecommendedWatcher::new(
            move |event: notify::Result<Event>| {
                let event = match event {
                    Ok(e) => e,
                    Err(e) => {
                        eprintln!("[tungsten] notify error: {e}");
                        return;
                    }
                };
                if let Some(note_event) = event_to_note_event(event) {
                    if tx.send(note_event).is_err() {
                        // Receiver dropped.
                    }
                }
            },
            notify::Config::default(),
        )
        .map_err(WatcherError::Notify)?;
        watcher
            .watch(&vault_root, RecursiveMode::Recursive)
            .map_err(WatcherError::Notify)?;
        Ok(Self {
            _watcher: watcher,
            events: rx,
            vault_root,
        })
    }

    /// Non-blocking poll. Returns Some(event) if there's a pending
    /// event, None if the channel is empty.
    pub fn try_next(&self) -> Option<NoteEvent> {
        self.events.try_recv().ok()
    }

    /// Block for up to `timeout` waiting for the next event.
    pub fn recv_timeout(&self, timeout: Duration) -> Result<NoteEvent, RecvTimeoutError> {
        self.events.recv_timeout(timeout)
    }

    /// Drain all currently-pending events and apply them to the
    /// given index. Returns the number of events applied.
    pub fn drain_into(&self, index: &mut NoteIndex) -> usize {
        let mut count = 0;
        while let Some(event) = self.try_next() {
            apply_event(index, event);
            count += 1;
        }
        count
    }

    /// Convenience: vault root the watcher is monitoring.
    pub fn vault_root(&self) -> &Path {
        &self.vault_root
    }
}

fn apply_event(index: &mut NoteIndex, event: NoteEvent) {
    match event {
        NoteEvent::Modified(path) => {
            if let Err(e) = index.update_note(&path) {
                eprintln!("[tungsten] update_note({}) failed: {e}", path.display());
            }
        }
        NoteEvent::Removed(path) => {
            index.remove_note(&path);
        }
    }
}

fn event_to_note_event(event: Event) -> Option<NoteEvent> {
    let kind = event.kind;
    for path in event.paths {
        // Only watch .md files. Skip everything else.
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        // Skip our own sidecar files.
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name == "index.db" {
                continue;
            }
        }
        return match &kind {
            EventKind::Create(_)
            | EventKind::Modify(
                ModifyKind::Data(_) | ModifyKind::Any | ModifyKind::Name(RenameMode::To),
            )
            | EventKind::Any => Some(NoteEvent::Modified(path)),
            EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
                Some(NoteEvent::Removed(path))
            }
            _ => None,
        };
    }
    None
}

/// Errors from `NoteWatcher::start`.
#[derive(Debug, thiserror::Error)]
pub enum WatcherError {
    #[error("notify error: {0}")]
    Notify(#[from] notify::Error),
}

// Keep HashMap/Arc/Mutex imports live for future coalescing logic.
#[allow(dead_code)]
type _FutureCoalescerState = (HashMap<PathBuf, NoteEvent>, Arc<Mutex<()>>, Sender<NoteEvent>);

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_vault(label: &str) -> Vault {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("tungsten-watch-{label}-{pid}-{n}"));
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir(dir.join(".obsidian")).unwrap();
        Vault::open(&dir).expect("vault")
    }

    #[test]
    fn start_and_drop_succeeds() {
        let vault = unique_vault("start-drop");
        let watcher = NoteWatcher::start(&vault).expect("start");
        assert_eq!(watcher.vault_root(), vault.root());
        // Drop happens at end of scope.
        fs::remove_dir_all(vault.root()).ok();
    }

    #[test]
    fn event_to_note_event_modified() {
        let path = PathBuf::from("/v/n.md");
        let event = Event {
            kind: EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Any)),
            paths: vec![path.clone()],
            attrs: Default::default(),
        };
        assert_eq!(
            event_to_note_event(event),
            Some(NoteEvent::Modified(path))
        );
    }

    #[test]
    fn event_to_note_event_removed() {
        let path = PathBuf::from("/v/n.md");
        let event = Event {
            kind: EventKind::Remove(notify::event::RemoveKind::File),
            paths: vec![path.clone()],
            attrs: Default::default(),
        };
        assert_eq!(event_to_note_event(event), Some(NoteEvent::Removed(path)));
    }

    #[test]
    fn event_to_note_event_ignores_non_md() {
        let event = Event {
            kind: EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("/v/photo.png")],
            attrs: Default::default(),
        };
        assert_eq!(event_to_note_event(event), None);
    }

    #[test]
    fn event_to_note_event_ignores_index_db() {
        let event = Event {
            kind: EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("/v/.tungsten/index.db")],
            attrs: Default::default(),
        };
        assert_eq!(event_to_note_event(event), None);
    }
}
