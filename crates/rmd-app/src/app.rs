//! Root component: toolbar, side-by-side editor and live preview.

use std::path::PathBuf;

use dioxus::desktop::tao::event::{Event, WindowEvent};
use dioxus::desktop::{WindowCloseBehaviour, use_wry_event_handler};
use dioxus::prelude::*;
use rfd::AsyncFileDialog;
use rmd_core::RenderConfig;

use crate::document::Document;
use crate::document_bar::DocumentBar;
use crate::editor::Editor;
use crate::export::ExportMenu;
use crate::find_bar::FindState;
use crate::icons::{Glyph, Icon};
use crate::instance::Requests;
use crate::library::Library;
use crate::library_panel::LibraryPanel;
use crate::local_files;
use crate::notice::{Notice, Status, StatusBar};
use crate::settings::{Settings, Theme, use_system_dark};
use crate::tabs::{TabBar, TabSet, Tabs};
use crate::watcher::use_file_watcher;

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

/// Files passed on the command line (e.g. "Open with…" from Explorer), opened
/// as tabs. Returns the live document, the tabs and any error to show.
fn initial_tabs() -> (Document, TabSet, Option<Notice>) {
    let mut live = Document::new();
    let mut set = TabSet::new();
    let mut notice = None;
    for path in std::env::args_os().skip(1).map(PathBuf::from) {
        match Document::open(&path) {
            Ok(doc) if set.find(&live, &path).is_none() => {
                set.open(&mut live, doc, (0.0, 0.0));
            }
            Ok(_) => {}
            Err(err) => {
                notice = Some(Notice::error(format!(
                    "Impossible d'ouvrir {} : {err}",
                    path.display()
                )));
            }
        }
    }
    (live, set, notice)
}

#[component]
pub fn App() -> Element {
    let initial = use_hook(initial_tabs);
    let doc = use_signal(|| initial.0.clone());
    let set = use_signal(|| initial.1.clone());
    let status = use_signal(|| initial.2.clone());
    let tabs = use_context_provider(|| Tabs::new(doc, set));
    let mut mode = use_signal(|| ViewMode::Split);
    let mut show_library = use_signal(|| true);
    use_context_provider(Library::open);
    let find = use_context_provider(FindState::new);
    let mut settings = use_context_provider(|| Signal::new(Settings::load()));
    let system_dark = use_system_dark();
    use_effect(move || settings.read().save());

    use_file_watcher(doc, status);

    // Files opened while the application is running ("Open with…", double
    // click in Explorer) arrive from the new process through the pipe.
    use_future(move || async move {
        let Some(mut requests) = try_consume_context::<Requests>().and_then(|r| r.take()) else {
            return;
        };
        while let Some(files) = requests.recv().await {
            let window = dioxus::desktop::window();
            window.set_minimized(false);
            window.set_visible(true);
            window.set_focus();
            for path in files {
                tabs.open_path(status, path).await;
            }
        }
    });

    use_wry_event_handler(move |event, _| {
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            spawn(close_window(tabs, status));
        }
    });

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
        let shift = event.modifiers().shift();
        let key = match event.key() {
            Key::Character(key) => key.to_lowercase(),
            Key::Tab => {
                fire(tabs.cycle(if shift { -1 } else { 1 }));
                event.prevent_default();
                return;
            }
            Key::PageDown | Key::PageUp => {
                fire(tabs.cycle(if event.key() == Key::PageUp { -1 } else { 1 }));
                event.prevent_default();
                return;
            }
            _ => return,
        };
        match key.as_str() {
            "s" if shift => fire(save_as(doc, status)),
            "s" => fire(save(doc, status)),
            "o" => fire(open(tabs, status)),
            "n" | "t" => fire(tabs.new_tab()),
            "w" => {
                let id = tabs.set.read().infos(&doc.read())[tabs.set.read().active_index()].id;
                fire(tabs.close(status, id));
            }
            "f" | "h" => {
                if mode() == ViewMode::Preview {
                    mode.set(ViewMode::Split);
                }
                find.show(key == "h");
            }
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
                AppMark {}
                button {
                    class: if show_library() { "active" },
                    title: "Afficher/masquer la bibliothèque",
                    onclick: move |_| show_library.toggle(),
                    Glyph { icon: Icon::Sidebar }
                    "Bibliothèque"
                }
                button { title: "Nouvel onglet (Ctrl+N)", onclick: move |_| { fire(tabs.new_tab()); }, "Nouveau" }
                button { title: "Ouvrir dans un onglet (Ctrl+O)", onclick: move |_| { fire(open(tabs, status)); }, "Ouvrir" }
                button { title: "Enregistrer (Ctrl+S)", onclick: move |_| { fire(save(doc, status)); }, "Enregistrer" }
                button { title: "Enregistrer sous (Ctrl+Maj+S)", onclick: move |_| { fire(save_as(doc, status)); }, "Enregistrer sous" }
                ExportMenu { doc, status }
                div { class: "spacer" }
                button {
                    title: "Thème : Système, Clair — Surface ou Sombre — Profondeur",
                    onclick: move |_| {
                        let next = settings.read().theme.next();
                        settings.write().theme = next;
                    },
                    Glyph {
                        icon: match theme {
                            Theme::System => Icon::Monitor,
                            Theme::Light => Icon::Sun,
                            Theme::Dark => Icon::Moon,
                        },
                    }
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
                    TabBar { tabs, status }
                    DocumentBar { doc, status }
                    main { class: "panes",
                        if current_mode != ViewMode::Preview {
                            Editor { doc, status }
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

fn markdown_dialog(doc: &Document) -> AsyncFileDialog {
    let dialog = AsyncFileDialog::new()
        .add_filter("Markdown", &["md", "markdown", "mdown", "mkd", "txt"])
        .add_filter("Tous les fichiers", &["*"]);
    match doc.base_dir() {
        Some(dir) => dialog.set_directory(dir),
        None => dialog,
    }
}

/// Lets the user pick one or more files, each opened in its own tab.
async fn open(tabs: Tabs, status: Status) {
    let dialog = markdown_dialog(&tabs.doc.read());
    let Some(files) = dialog.pick_files().await else {
        return;
    };
    for file in files {
        tabs.open_path(status, file.path().to_path_buf()).await;
    }
}

/// Opens `path` (e.g. from the library) in a tab, or shows its tab if it is
/// already open.
pub async fn open_path(status: Status, path: PathBuf) {
    if let Some(tabs) = try_consume_context::<Tabs>() {
        tabs.open_path(status, path).await;
    }
}

/// Saves to the current path, or asks for one. Returns `true` on success.
pub async fn save(mut doc: Signal<Document>, mut status: Status) -> bool {
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

/// Handles the window close button: asks about unsaved changes in every tab first.
///
/// The window is configured to hide on close, so it is shown again while the
/// question is asked; it only really closes once the user agreed.
async fn close_window(tabs: Tabs, status: Status) {
    let window = dioxus::desktop::window();
    if !tabs.set.read().dirty(&tabs.doc.read()).is_empty() {
        window.set_visible(true);
        window.set_focus();
        if !tabs.confirm_close_all(status).await {
            return;
        }
    }
    window.set_close_behavior(WindowCloseBehaviour::WindowCloses);
    window.close();
}

/// Caretta "Markdown editor" monogram (interface version, 24 px line icon).
///
/// Inlined rather than loaded as an image so the stroke can use
/// `currentColor` and follow the light/dark theme.
#[component]
fn AppMark() -> Element {
    rsx! {
        svg {
            class: "app-mark",
            width: "22",
            height: "22",
            view_box: "0 0 24 24",
            role: "img",
            "aria-label": "Rust Markdown",
            path {
                d: "M2 16 V3 L7 10 L12 3 V21 H16 A6 6.5 0 0 0 16 8 H12",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.5",
                stroke_linecap: "round",
                stroke_linejoin: "round",
            }
        }
    }
}
