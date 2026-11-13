//! Reacts to Markdown files changed by other programs.
//!
//! Folders are watched rather than files: many editors save through a
//! temporary file and a rename, which a file-level watch would miss. Watched
//! folders are the open document's and those of every library document.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use dioxus::prelude::*;
use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};
use rfd::{AsyncMessageDialog, MessageButtons, MessageDialogResult, MessageLevel};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use crate::document::{DiskState, Document};
use crate::library::Library;
use crate::notice::{Notice, Status};

/// Editors often write a file in several steps: wait for things to settle.
const DEBOUNCE: Duration = Duration::from_millis(300);

struct FolderWatcher {
    debouncer: Debouncer<RecommendedWatcher>,
    folders: HashSet<PathBuf>,
}

impl FolderWatcher {
    fn start() -> Option<(Self, UnboundedReceiver<Vec<PathBuf>>)> {
        let (sender, receiver) = unbounded_channel();
        let debouncer = new_debouncer(DEBOUNCE, move |result: DebounceEventResult| {
            if let Ok(events) = result {
                let _ = sender.send(events.into_iter().map(|event| event.path).collect());
            }
        })
        .ok()?;
        let watcher = Self {
            debouncer,
            folders: HashSet::new(),
        };
        Some((watcher, receiver))
    }

    /// Watches exactly `wanted` (non-recursively), adding and removing as needed.
    fn set_folders(&mut self, wanted: HashSet<PathBuf>) {
        for gone in self.folders.difference(&wanted) {
            let _ = self.debouncer.watcher().unwatch(gone);
        }
        for new in wanted.difference(&self.folders) {
            // A folder that cannot be watched (deleted, no access) is skipped.
            let _ = self
                .debouncer
                .watcher()
                .watch(new, RecursiveMode::NonRecursive);
        }
        self.folders = wanted;
    }
}

/// Windows paths are case-insensitive.
fn same_path(a: &Path, b: &Path) -> bool {
    let normalize = |path: &Path| {
        std::path::absolute(path)
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .to_lowercase()
    };
    normalize(a) == normalize(b)
}

/// Watches files on disk for the lifetime of the calling component.
pub fn use_file_watcher(doc: Signal<Document>, status: Status) {
    let library = use_context::<Library>();
    let watcher = use_hook(|| {
        FolderWatcher::start().map(|(watcher, receiver)| {
            (
                Rc::new(RefCell::new(watcher)),
                Rc::new(RefCell::new(Some(receiver))),
            )
        })
    });
    let open_path = use_memo(move || doc.read().path().map(Path::to_path_buf));
    // Set while the "reload?" question is on screen, to ask only once.
    let asking = use_signal(|| false);

    let lib = library.clone();
    let folders_watcher = watcher.clone();
    use_effect(move || {
        let Some((watcher, _)) = &folders_watcher else {
            return;
        };
        let mut folders: HashSet<PathBuf> = lib
            .read(|store| store.document_directories())
            .unwrap_or_default()
            .into_iter()
            .collect();
        if let Some(parent) = open_path().as_deref().and_then(Path::parent) {
            folders.insert(parent.to_path_buf());
        }
        watcher.borrow_mut().set_folders(folders);
    });

    use_future(move || {
        let receiver = watcher
            .as_ref()
            .and_then(|(_, receiver)| receiver.borrow_mut().take());
        let library = library.clone();
        async move {
            let Some(mut receiver) = receiver else { return };
            while let Some(paths) = receiver.recv().await {
                library.sync_files(&paths);
                let open = open_path();
                let touches_open = open
                    .as_deref()
                    .is_some_and(|open| paths.iter().any(|path| same_path(path, open)));
                if touches_open {
                    on_open_file_changed(doc, status, asking).await;
                }
            }
        }
    });
}

async fn on_open_file_changed(
    mut doc: Signal<Document>,
    mut status: Status,
    mut asking: Signal<bool>,
) {
    if asking() {
        return;
    }
    let name = doc.read().display_name();
    let state = doc.read().disk_state();
    match state {
        DiskState::Unchanged => {}
        DiskState::Missing => status.set(Some(Notice::error(format!(
            "« {name} » a été supprimé ou déplacé par un autre programme. \
             Enregistrez-le (Ctrl+S) pour le recréer."
        )))),
        DiskState::Modified(content) if !doc.read().is_dirty() => {
            doc.write().reload(content);
            status.set(Some(Notice::info(format!(
                "« {name} » a été modifié par un autre programme : rechargé."
            ))));
        }
        DiskState::Modified(_) => {
            asking.set(true);
            let answer = AsyncMessageDialog::new()
                .set_level(MessageLevel::Warning)
                .set_title("Fichier modifié par un autre programme")
                .set_description(format!(
                    "« {name} » a été modifié en dehors de l'application, et vous avez \
                     aussi des modifications non enregistrées.\n\n\
                     Recharger la version du disque ? Vos modifications seront perdues.\n\
                     (Non : garder votre version ; l'enregistrer écrasera l'autre.)"
                ))
                .set_buttons(MessageButtons::YesNo)
                .show()
                .await;
            asking.set(false);
            if answer == MessageDialogResult::Yes {
                // Read again: the file may have changed while the question was shown.
                let latest = doc.read().disk_state();
                if let DiskState::Modified(content) = latest {
                    doc.write().reload(content);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_paths_case_insensitively() {
        assert!(same_path(
            Path::new(r"C:\Notes\A.md"),
            Path::new(r"c:\notes\a.MD")
        ));
        assert!(!same_path(
            Path::new(r"C:\Notes\A.md"),
            Path::new(r"C:\Notes\B.md")
        ));
    }
}
