//! Root component: toolbar, side-by-side editor and live preview.

use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use rfd::{AsyncFileDialog, AsyncMessageDialog, MessageButtons, MessageDialogResult, MessageLevel};
use rmd_core::RenderConfig;

use crate::document::Document;
use crate::document_bar::DocumentBar;
use crate::editor::Editor;
use crate::export::ExportMenu;
use crate::library::Library;
use crate::library_panel::LibraryPanel;
use crate::local_files;
use crate::notice::{Notice, Status, StatusBar};
use crate::settings::{Settings, use_system_dark};

const PREVIEW_SCRIPT: &str = include_str!("preview.js");
const EDITOR_SCRIPT: &str = include_str!("editor.js");

/// Which panes are visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Editor,
    Split,
    Preview,
}

impl ViewMode {
    const ALL: [ViewMode; 3] = [ViewMode::Editor, ViewMode::Split, ViewMode::Preview];

    fn label(self) -> &'static str {
        match self {
            ViewMode::Editor => "Éditeur",
            ViewMode::Split => "Côte à côte",
            ViewMode::Preview => "Aperçu",
        }
    }
}

/// Document passed on the command line (e.g. "Open with…" from Explorer).
fn initial_document() -> (Document, Option<Notice>) {
    match std::env::args_os().nth(1).map(PathBuf::from) {
        None => (Document::new(), None),
        Some(path) => match Document::open(&path) {
            Ok(doc) => (doc, None),
            Err(err) => (
                Document::new(),
                Some(Notice::error(format!(
                    "Impossible d'ouvrir {} : {err}",
                    path.display()
                ))),
            ),
        },
    }
}

#[component]
pub fn App() -> Element {
    let initial = use_hook(initial_document);
    let doc = use_signal(|| initial.0.clone());
    let status = use_signal(|| initial.1.clone());
    let mut mode = use_signal(|| ViewMode::Split);
    let mut show_library = use_signal(|| true);
    use_context_provider(Library::open);
    let mut settings = use_context_provider(|| Signal::new(Settings::load()));
    let system_dark = use_system_dark();
    use_effect(move || settings.read().save());

    let html = use_memo(move || {
        let doc = doc.read();
        let config = RenderConfig {
            image_url_rewriter: Some(local_files::image_rewriter(doc.base_dir())),
            source_positions: true,
            ..Default::default()
        };
        rmd_core::render_html_with(&doc.content, &config)
    });

    use_effect(move || {
        let doc = doc.read();
        let marker = if doc.is_dirty() { "● " } else { "" };
        dioxus::desktop::window()
            .set_title(&format!("{marker}{} — Rust Markdown", doc.display_name()));
    });

    // In-document links (footnotes…) scroll the preview instead of opening a
    // browser; editor and preview scroll together.
    use_effect(|| {
        let _ = document::eval(PREVIEW_SCRIPT);
        let _ = document::eval(EDITOR_SCRIPT);
    });

    let on_keydown = move |event: KeyboardEvent| {
        if !event.modifiers().ctrl() {
            return;
        }
        let Key::Character(key) = event.key() else {
            return;
        };
        let shift = event.modifiers().shift();
        match key.to_lowercase().as_str() {
            "s" if shift => fire(save_as(doc, status)),
            "s" => fire(save(doc, status)),
            "o" => fire(open(doc, status)),
            "n" => fire(new_document(doc, status)),
            _ => return,
        };
        event.prevent_default();
    };

    let current_mode = mode();
    let theme = settings.read().theme;
    let dark = theme.is_dark(system_dark());
    let code_css = rmd_core::highlight_css(theme.code_theme(system_dark()));
    rsx! {
        style { "{code_css}" }
        div {
            class: "app",
            "data-theme": if dark { "dark" } else { "light" },
            onkeydown: on_keydown,
            header { class: "toolbar",
                button {
                    class: if show_library() { "active" },
                    title: "Afficher/masquer la bibliothèque",
                    onclick: move |_| show_library.toggle(),
                    "Bibliothèque"
                }
                button { title: "Nouveau (Ctrl+N)", onclick: move |_| { fire(new_document(doc, status)); }, "Nouveau" }
                button { title: "Ouvrir (Ctrl+O)", onclick: move |_| { fire(open(doc, status)); }, "Ouvrir" }
                button { title: "Enregistrer (Ctrl+S)", onclick: move |_| { fire(save(doc, status)); }, "Enregistrer" }
                button { title: "Enregistrer sous (Ctrl+Maj+S)", onclick: move |_| { fire(save_as(doc, status)); }, "Enregistrer sous" }
                ExportMenu { doc, status }
                div { class: "spacer" }
                button {
                    title: "Thème (Système → Clair → Sombre)",
                    onclick: move |_| {
                        let next = settings.read().theme.next();
                        settings.write().theme = next;
                    },
                    "{theme.label()}"
                }
                div { class: "segmented",
                    for view in ViewMode::ALL {
                        button {
                            class: if view == current_mode { "active" },
                            onclick: move |_| mode.set(view),
                            "{view.label()}"
                        }
                    }
                }
            }
            div { class: "body",
                if show_library() {
                    LibraryPanel { doc, status }
                }
                div { class: "workspace",
                    DocumentBar { doc, status }
                    main { class: "panes",
                        if current_mode != ViewMode::Preview {
                            Editor { doc }
                        }
                        if current_mode != ViewMode::Editor {
                            article { class: "preview",
                                div { class: "markdown-body", dangerous_inner_html: "{html}" }
                            }
                        }
                    }
                }
            }
            StatusBar { status }
        }
    }
}

/// Asks what to do with unsaved changes. Returns `true` if the caller may proceed.
async fn confirm_discard(doc: Signal<Document>, status: Status) -> bool {
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
        MessageDialogResult::Yes => save(doc, status).await,
        MessageDialogResult::No => true,
        _ => false,
    }
}

fn markdown_dialog(doc: &Document) -> AsyncFileDialog {
    let dialog = AsyncFileDialog::new()
        .add_filter("Markdown", &["md", "markdown", "mdown", "mkd", "txt"])
        .add_filter("Tous les fichiers", &["*"]);
    match doc.base_dir() {
        Some(dir) => dialog.set_directory(dir),
        None => dialog,
    }
}

async fn new_document(mut doc: Signal<Document>, status: Status) {
    if confirm_discard(doc, status).await {
        doc.set(Document::new());
    }
}

async fn open(doc: Signal<Document>, status: Status) {
    if !confirm_discard(doc, status).await {
        return;
    }
    let dialog = markdown_dialog(&doc.read());
    let Some(file) = dialog.pick_file().await else {
        return;
    };
    load(doc, status, file.path());
}

/// Opens `path` (e.g. from the library), asking first about unsaved changes.
pub async fn open_path(doc: Signal<Document>, status: Status, path: PathBuf) {
    if doc.read().path() == Some(path.as_path()) {
        return;
    }
    if confirm_discard(doc, status).await {
        load(doc, status, &path);
    }
}

fn load(mut doc: Signal<Document>, mut status: Status, path: &Path) {
    match Document::open(path) {
        Ok(opened) => {
            doc.set(opened);
            status.set(None);
        }
        Err(err) => status.set(Some(Notice::error(format!(
            "Impossible d'ouvrir le fichier : {err}"
        )))),
    }
}

/// Saves to the current path, or asks for one. Returns `true` on success.
async fn save(mut doc: Signal<Document>, mut status: Status) -> bool {
    if doc.read().path().is_none() {
        return save_as(doc, status).await;
    }
    let result = doc.write().save();
    report_save(result, doc, &mut status)
}

async fn save_as(mut doc: Signal<Document>, mut status: Status) -> bool {
    let dialog = markdown_dialog(&doc.read()).set_file_name(doc.read().display_name());
    let Some(file) = dialog.save_file().await else {
        return false;
    };
    let mut path = file.path().to_path_buf();
    if path.extension().is_none() {
        path.set_extension("md");
    }
    let result = doc.write().save_as(path);
    report_save(result, doc, &mut status)
}

fn report_save(result: std::io::Result<()>, doc: Signal<Document>, status: &mut Status) -> bool {
    match result {
        Ok(()) => {
            status.set(None);
            reindex(&doc.read());
            true
        }
        Err(err) => {
            status.set(Some(Notice::error(format!(
                "Échec de l'enregistrement : {err}"
            ))));
            false
        }
    }
}

/// Keeps the library index in sync after the document was written to disk.
fn reindex(doc: &Document) {
    let (Some(path), Some(library)) = (doc.path(), try_consume_context::<Library>()) else {
        return;
    };
    let _ = library.write(|store| match store.document_by_path(path)? {
        Some(entry) => store.reindex(entry.id),
        None => Ok(()),
    });
}

/// Runs an async action in the background, ignoring its result.
fn fire<T: 'static>(action: impl Future<Output = T> + 'static) {
    spawn(async move {
        action.await;
    });
}
