//! Root component: toolbar, side-by-side editor and live preview.

use std::path::PathBuf;

use dioxus::prelude::*;
use rfd::{AsyncFileDialog, AsyncMessageDialog, MessageButtons, MessageDialogResult, MessageLevel};
use rmd_core::RenderConfig;

use crate::document::Document;
use crate::local_files;

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
fn initial_document() -> (Document, Option<String>) {
    match std::env::args_os().nth(1).map(PathBuf::from) {
        None => (Document::new(), None),
        Some(path) => match Document::open(&path) {
            Ok(doc) => (doc, None),
            Err(err) => (
                Document::new(),
                Some(format!("Impossible d'ouvrir {} : {err}", path.display())),
            ),
        },
    }
}

#[component]
pub fn App() -> Element {
    let initial = use_hook(initial_document);
    let mut doc = use_signal(|| initial.0.clone());
    let mut status = use_signal(|| initial.1.clone());
    let mut mode = use_signal(|| ViewMode::Split);

    let html = use_memo(move || {
        let doc = doc.read();
        let config = RenderConfig {
            image_url_rewriter: Some(local_files::image_rewriter(doc.base_dir())),
        };
        rmd_core::render_html_with(&doc.content, &config)
    });

    use_effect(move || {
        let doc = doc.read();
        let marker = if doc.is_dirty() { "● " } else { "" };
        dioxus::desktop::window()
            .set_title(&format!("{marker}{} — Rust Markdown", doc.display_name()));
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
    rsx! {
        div { class: "app", onkeydown: on_keydown,
            header { class: "toolbar",
                button { title: "Nouveau (Ctrl+N)", onclick: move |_| { fire(new_document(doc, status)); }, "Nouveau" }
                button { title: "Ouvrir (Ctrl+O)", onclick: move |_| { fire(open(doc, status)); }, "Ouvrir" }
                button { title: "Enregistrer (Ctrl+S)", onclick: move |_| { fire(save(doc, status)); }, "Enregistrer" }
                button { title: "Enregistrer sous (Ctrl+Maj+S)", onclick: move |_| { fire(save_as(doc, status)); }, "Enregistrer sous" }
                div { class: "spacer" }
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
            main { class: "panes",
                if current_mode != ViewMode::Preview {
                    textarea {
                        class: "editor",
                        spellcheck: "false",
                        placeholder: "Écrivez du Markdown ici…",
                        value: "{doc.read().content}",
                        oninput: move |event| doc.write().content = event.value(),
                    }
                }
                if current_mode != ViewMode::Editor {
                    article { class: "preview",
                        div { class: "markdown-body", dangerous_inner_html: "{html}" }
                    }
                }
            }
            if let Some(message) = status() {
                footer { class: "status",
                    span { "{message}" }
                    button { class: "link", onclick: move |_| status.set(None), "✕" }
                }
            }
        }
    }
}

/// Asks what to do with unsaved changes. Returns `true` if the caller may proceed.
async fn confirm_discard(doc: Signal<Document>, status: Signal<Option<String>>) -> bool {
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

async fn new_document(mut doc: Signal<Document>, status: Signal<Option<String>>) {
    if confirm_discard(doc, status).await {
        doc.set(Document::new());
    }
}

async fn open(mut doc: Signal<Document>, mut status: Signal<Option<String>>) {
    if !confirm_discard(doc, status).await {
        return;
    }
    let dialog = markdown_dialog(&doc.read());
    let Some(file) = dialog.pick_file().await else {
        return;
    };
    match Document::open(file.path()) {
        Ok(opened) => {
            doc.set(opened);
            status.set(None);
        }
        Err(err) => status.set(Some(format!("Impossible d'ouvrir le fichier : {err}"))),
    }
}

/// Saves to the current path, or asks for one. Returns `true` on success.
async fn save(mut doc: Signal<Document>, mut status: Signal<Option<String>>) -> bool {
    if doc.read().path().is_none() {
        return save_as(doc, status).await;
    }
    let result = doc.write().save();
    report_save(result, &mut status)
}

async fn save_as(mut doc: Signal<Document>, mut status: Signal<Option<String>>) -> bool {
    let dialog = markdown_dialog(&doc.read()).set_file_name(doc.read().display_name());
    let Some(file) = dialog.save_file().await else {
        return false;
    };
    let mut path = file.path().to_path_buf();
    if path.extension().is_none() {
        path.set_extension("md");
    }
    let result = doc.write().save_as(path);
    report_save(result, &mut status)
}

fn report_save(result: std::io::Result<()>, status: &mut Signal<Option<String>>) -> bool {
    match result {
        Ok(()) => {
            status.set(None);
            true
        }
        Err(err) => {
            status.set(Some(format!("Échec de l'enregistrement : {err}")));
            false
        }
    }
}

/// Runs an async action in the background, ignoring its result.
fn fire<T: 'static>(action: impl Future<Output = T> + 'static) {
    spawn(async move {
        action.await;
    });
}
