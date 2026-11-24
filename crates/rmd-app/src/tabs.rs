//! Open documents, one tab each.
//!
//! The active document lives in its own `Signal<Document>`, which every pane
//! (editor, preview, find bar, document bar, watcher) works on. The other
//! tabs are parked in [`TabSet`]; switching tabs swaps documents between the
//! two, so the panes never need to know about tabs.

use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use rfd::{AsyncMessageDialog, MessageButtons, MessageDialogResult, MessageLevel};

use crate::document::Document;
use crate::icons::{Glyph, Icon};
use crate::notice::{Notice, Status};

/// Identifies a tab for its whole life (indices shift when tabs close).
pub type TabId = u64;

#[derive(Debug, Clone, PartialEq)]
struct Tab {
    id: TabId,
    /// The parked document; meaningless for the active tab (see module docs).
    doc: Document,
    /// Editor and preview scroll positions, restored when coming back.
    scroll: (f64, f64),
}

/// What the tab bar shows for a tab.
#[derive(Debug, Clone, PartialEq)]
pub struct TabInfo {
    pub id: TabId,
    pub name: String,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub active: bool,
}

/// The tabs, in order. There is always at least one.
#[derive(Debug, Clone, PartialEq)]
pub struct TabSet {
    tabs: Vec<Tab>,
    active: usize,
    next_id: TabId,
}

impl TabSet {
    /// A single tab, whose document is the live one.
    pub fn new() -> Self {
        Self {
            tabs: vec![Tab {
                id: 0,
                doc: Document::new(),
                scroll: (0.0, 0.0),
            }],
            active: 0,
            next_id: 1,
        }
    }

    pub fn count(&self) -> usize {
        self.tabs.len()
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn index_of(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == id)
    }

    fn doc<'a>(&'a self, live: &'a Document, index: usize) -> &'a Document {
        if index == self.active {
            live
        } else {
            &self.tabs[index].doc
        }
    }

    /// Tab showing `path`, if any (Windows paths are case-insensitive).
    pub fn find(&self, live: &Document, path: &Path) -> Option<usize> {
        (0..self.tabs.len()).find(|&index| {
            self.doc(live, index)
                .path()
                .is_some_and(|open| same_path(open, path))
        })
    }

    pub fn infos(&self, live: &Document) -> Vec<TabInfo> {
        (0..self.tabs.len())
            .map(|index| {
                let doc = self.doc(live, index);
                TabInfo {
                    id: self.tabs[index].id,
                    name: doc.display_name(),
                    path: doc.path().map(Path::to_path_buf),
                    dirty: doc.is_dirty(),
                    active: index == self.active,
                }
            })
            .collect()
    }

    /// Indices of tabs with unsaved changes.
    pub fn dirty(&self, live: &Document) -> Vec<usize> {
        (0..self.tabs.len())
            .filter(|&index| self.doc(live, index).is_dirty())
            .collect()
    }

    /// Makes `index` the active tab, parking the live document.
    /// `scroll` is the live document's scroll position; the new tab's is returned.
    pub fn activate(
        &mut self,
        live: &mut Document,
        index: usize,
        scroll: (f64, f64),
    ) -> (f64, f64) {
        if index == self.active || index >= self.tabs.len() {
            return self.tabs[self.active].scroll;
        }
        let previous = self.active;
        self.tabs[previous].scroll = scroll;
        std::mem::swap(live, &mut self.tabs[previous].doc);
        std::mem::swap(live, &mut self.tabs[index].doc);
        self.active = index;
        self.tabs[index].scroll
    }

    /// Shows `doc` in a new tab after the active one — or in place of the
    /// active tab when that one is an untouched blank document.
    /// Returns the index of the tab now showing it.
    pub fn open(&mut self, live: &mut Document, doc: Document, scroll: (f64, f64)) -> usize {
        if *live == Document::new() {
            *live = doc;
            self.tabs[self.active].scroll = (0.0, 0.0);
            return self.active;
        }
        let index = self.active + 1;
        self.tabs.insert(
            index,
            Tab {
                id: self.next_id,
                doc,
                scroll: (0.0, 0.0),
            },
        );
        self.next_id += 1;
        self.activate(live, index, scroll);
        index
    }

    /// Opens an empty document in a new tab.
    pub fn new_tab(&mut self, live: &mut Document, scroll: (f64, f64)) {
        let index = self.active + 1;
        self.tabs.insert(
            index,
            Tab {
                id: self.next_id,
                doc: Document::new(),
                scroll: (0.0, 0.0),
            },
        );
        self.next_id += 1;
        self.activate(live, index, scroll);
    }

    /// Closes the tab at `index` without asking anything. Closing the last
    /// tab leaves a blank document. Returns the scroll position to restore
    /// when the active document changed.
    pub fn close(&mut self, live: &mut Document, index: usize) -> Option<(f64, f64)> {
        if index >= self.tabs.len() {
            return None;
        }
        if self.tabs.len() == 1 {
            *live = Document::new();
            self.tabs[0].scroll = (0.0, 0.0);
            return Some((0.0, 0.0));
        }
        if index != self.active {
            self.tabs.remove(index);
            if index < self.active {
                self.active -= 1;
            }
            return None;
        }
        // The neighbour on the right takes the place, as in browsers.
        let next = if index + 1 < self.tabs.len() {
            index + 1
        } else {
            index - 1
        };
        let scroll = self.activate(live, next, (0.0, 0.0));
        self.tabs.remove(index);
        if index < self.active {
            self.active -= 1;
        }
        Some(scroll)
    }
}

impl Default for TabSet {
    fn default() -> Self {
        Self::new()
    }
}

/// Windows paths are case-insensitive.
pub fn same_path(a: &Path, b: &Path) -> bool {
    let normalize = |path: &Path| {
        std::path::absolute(path)
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .to_lowercase()
    };
    normalize(a) == normalize(b)
}

/// Handle on the tabs, shared through the context: the live document plus
/// the parked ones.
#[derive(Clone, Copy, PartialEq)]
pub struct Tabs {
    pub doc: Signal<Document>,
    pub set: Signal<TabSet>,
}

/// Reads the editor and preview scroll positions.
async fn scroll_position() -> (f64, f64) {
    let script = "return [document.querySelector('textarea.editor')?.scrollTop || 0, \
                  document.querySelector('.preview')?.scrollTop || 0];";
    match document::eval(script).join::<Vec<f64>>().await {
        Ok(position) if position.len() == 2 => (position[0], position[1]),
        _ => (0.0, 0.0),
    }
}

/// Restores scroll positions once the new document has been rendered.
fn restore_scroll((editor, preview): (f64, f64)) {
    let _ = document::eval(&format!(
        "requestAnimationFrame(() => requestAnimationFrame(() => {{ \
           const editor = document.querySelector('textarea.editor'); \
           const preview = document.querySelector('.preview'); \
           if (editor) editor.scrollTop = {editor}; \
           if (preview) preview.scrollTop = {preview}; \
         }}));"
    ));
}

impl Tabs {
    pub fn new(doc: Signal<Document>, set: Signal<TabSet>) -> Self {
        Self { doc, set }
    }

    pub async fn activate(mut self, index: usize) {
        if index == self.set.read().active_index() || index >= self.set.read().count() {
            return;
        }
        let scroll = scroll_position().await;
        let restored = self
            .set
            .write()
            .activate(&mut self.doc.write(), index, scroll);
        restore_scroll(restored);
    }

    /// Activates the next (`step` = 1) or previous (`step` = -1) tab, cycling.
    pub async fn cycle(self, step: isize) {
        let (len, active) = {
            let set = self.set.read();
            (set.count() as isize, set.active_index() as isize)
        };
        self.activate((active + step).rem_euclid(len) as usize)
            .await;
    }

    pub async fn new_tab(mut self) {
        let scroll = scroll_position().await;
        self.set.write().new_tab(&mut self.doc.write(), scroll);
        restore_scroll((0.0, 0.0));
    }

    /// Opens `path`, or switches to the tab already showing it.
    pub async fn open_path(mut self, mut status: Status, path: PathBuf) {
        let existing = self.set.read().find(&self.doc.read(), &path);
        if let Some(index) = existing {
            self.activate(index).await;
            return;
        }
        match Document::open(&path) {
            Ok(doc) => {
                let scroll = scroll_position().await;
                self.set.write().open(&mut self.doc.write(), doc, scroll);
                restore_scroll((0.0, 0.0));
                status.set(None);
            }
            Err(err) => status.set(Some(Notice::error(format!(
                "Impossible d'ouvrir {} : {err}",
                path.display()
            )))),
        }
    }

    /// Closes a tab, asking first about unsaved changes.
    /// Returns `false` if the user cancelled.
    pub async fn close(mut self, status: Status, id: TabId) -> bool {
        let Some(index) = self.set.read().index_of(id) else {
            return true;
        };
        let dirty = self.set.read().dirty(&self.doc.read()).contains(&index);
        if dirty {
            // Show the document the question is about.
            self.activate(index).await;
            if !confirm_discard(self.doc, status).await {
                return false;
            }
        }
        let Some(index) = self.set.read().index_of(id) else {
            return true;
        };
        let restored = self.set.write().close(&mut self.doc.write(), index);
        if let Some(scroll) = restored {
            restore_scroll(scroll);
        }
        true
    }

    /// Goes through every tab with unsaved changes. Returns `false` if the
    /// user cancelled (the window must then stay open).
    pub async fn confirm_close_all(self, status: Status) -> bool {
        let dirty: Vec<TabId> = self
            .set
            .read()
            .infos(&self.doc.read())
            .into_iter()
            .filter(|info| info.dirty)
            .map(|info| info.id)
            .collect();
        for id in dirty {
            let Some(index) = self.set.read().index_of(id) else {
                continue;
            };
            // Show the document the question is about.
            self.activate(index).await;
            if !confirm_discard(self.doc, status).await {
                return false;
            }
        }
        true
    }
}

/// Asks what to do with unsaved changes. Returns `true` if the caller may proceed.
pub async fn confirm_discard(doc: Signal<Document>, status: Status) -> bool {
    if !doc.read().is_dirty() {
        return true;
    }
    let name = doc.read().display_name();
    let answer = AsyncMessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title("Modifications non enregistrées")
        .set_description(format!("Enregistrer les modifications de « {name} » ?"))
        .set_buttons(MessageButtons::YesNoCancel)
        .show()
        .await;
    match answer {
        MessageDialogResult::Yes => crate::app::save(doc, status).await,
        MessageDialogResult::No => true,
        _ => false,
    }
}

/// The row of tabs above the document.
#[component]
pub fn TabBar(tabs: Tabs, status: Status) -> Element {
    let infos = tabs.set.read().infos(&tabs.doc.read());
    rsx! {
        nav { class: "tabbar", role: "tablist",
            for (index, info) in infos.into_iter().enumerate() {
                div {
                    key: "{info.id}",
                    class: if info.active { "tab active" } else { "tab" },
                    role: "tab",
                    "aria-selected": "{info.active}",
                    title: info.path.as_deref().map(|path| path.display().to_string()).unwrap_or_else(|| info.name.clone()),
                    onclick: move |_| { spawn(tabs.activate(index)); },
                    // Middle click closes, as in browsers.
                    onmousedown: move |event| {
                        if event.trigger_button() == Some(dioxus::html::input_data::MouseButton::Auxiliary) {
                            event.prevent_default();
                            spawn(async move { tabs.close(status, info.id).await; });
                        }
                    },
                    span { class: "tab-name", "{info.name}" }
                    if info.dirty {
                        span { class: "tab-dirty", title: "Modifications non enregistrées", "●" }
                    }
                    button {
                        class: "icon tab-close",
                        title: "Fermer l'onglet (Ctrl+W)",
                        onclick: move |event| {
                            event.stop_propagation();
                            spawn(async move { tabs.close(status, info.id).await; });
                        },
                        Glyph { icon: Icon::Close, size: 12 }
                    }
                }
            }
            button {
                class: "icon tab-new",
                title: "Nouvel onglet (Ctrl+N)",
                onclick: move |_| { spawn(tabs.new_tab()); },
                Glyph { icon: Icon::Plus, size: 14 }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn file(dir: &Path, name: &str, text: &str) -> Document {
        let path = dir.join(name);
        fs::write(&path, text).unwrap();
        Document::open(path).unwrap()
    }

    fn names(set: &TabSet, live: &Document) -> Vec<String> {
        set.infos(live).into_iter().map(|info| info.name).collect()
    }

    #[test]
    fn first_file_replaces_the_blank_tab() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = TabSet::new();
        let mut live = Document::new();
        assert_eq!(
            set.open(&mut live, file(dir.path(), "a.md", "A"), (0.0, 0.0)),
            0
        );
        assert_eq!(names(&set, &live), ["a.md"]);
        assert_eq!(
            set.open(&mut live, file(dir.path(), "b.md", "B"), (0.0, 0.0)),
            1
        );
        assert_eq!(names(&set, &live), ["a.md", "b.md"]);
        assert_eq!(live.content, "B");
    }

    #[test]
    fn switching_keeps_each_document_and_its_scroll() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = TabSet::new();
        let mut live = Document::new();
        set.open(&mut live, file(dir.path(), "a.md", "A"), (0.0, 0.0));
        live.content.push_str(" edited");
        set.open(&mut live, file(dir.path(), "b.md", "B"), (12.0, 34.0));

        assert_eq!(set.activate(&mut live, 0, (5.0, 6.0)), (12.0, 34.0));
        assert_eq!(live.content, "A edited");
        assert_eq!(set.dirty(&live), [0]);
        assert_eq!(set.activate(&mut live, 1, (0.0, 0.0)), (5.0, 6.0));
        assert_eq!(live.content, "B");
        assert_eq!(set.dirty(&live), [0]);
    }

    #[test]
    fn finds_open_files_case_insensitively() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = TabSet::new();
        let mut live = Document::new();
        set.open(&mut live, file(dir.path(), "a.md", "A"), (0.0, 0.0));
        set.open(&mut live, file(dir.path(), "b.md", "B"), (0.0, 0.0));
        let upper = PathBuf::from(dir.path().join("A.MD").to_string_lossy().to_uppercase());
        assert_eq!(set.find(&live, &upper), Some(0));
        assert_eq!(set.find(&live, &dir.path().join("b.md")), Some(1));
        assert_eq!(set.find(&live, &dir.path().join("c.md")), None);
    }

    #[test]
    fn new_tabs_open_next_to_the_active_one() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = TabSet::new();
        let mut live = Document::new();
        set.open(&mut live, file(dir.path(), "a.md", "A"), (0.0, 0.0));
        set.open(&mut live, file(dir.path(), "b.md", "B"), (0.0, 0.0));
        set.activate(&mut live, 0, (0.0, 0.0));
        set.new_tab(&mut live, (0.0, 0.0));
        assert_eq!(names(&set, &live), ["a.md", "Sans titre", "b.md"]);
        assert_eq!(set.active_index(), 1);
    }

    #[test]
    fn closing_moves_to_a_neighbour() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = TabSet::new();
        let mut live = Document::new();
        for name in ["a.md", "b.md", "c.md"] {
            set.open(&mut live, file(dir.path(), name, name), (0.0, 0.0));
        }
        // Active: c (last) → closing it activates b.
        assert!(set.close(&mut live, 2).is_some());
        assert_eq!(live.content, "b.md");
        // Closing an inactive tab before the active one keeps b active.
        assert!(set.close(&mut live, 0).is_none());
        assert_eq!(names(&set, &live), ["b.md"]);
        assert_eq!(set.active_index(), 0);
        assert_eq!(live.content, "b.md");
        // The last tab becomes blank instead of disappearing.
        set.close(&mut live, 0);
        assert_eq!(set.count(), 1);
        assert_eq!(live, Document::new());
    }

    #[test]
    fn closing_the_active_middle_tab_activates_the_right_one() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = TabSet::new();
        let mut live = Document::new();
        for name in ["a.md", "b.md", "c.md"] {
            set.open(&mut live, file(dir.path(), name, name), (0.0, 0.0));
        }
        set.activate(&mut live, 1, (0.0, 0.0));
        set.close(&mut live, 1);
        assert_eq!(names(&set, &live), ["a.md", "c.md"]);
        assert_eq!(set.active_index(), 1);
        assert_eq!(live.content, "c.md");
    }
}
