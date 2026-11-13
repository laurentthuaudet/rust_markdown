//! Markdown source editor: textarea, line-number gutter and context menu.

use dioxus::desktop::muda::{CheckMenuItem, ContextMenu, Menu, MenuEvent, PredefinedMenuItem};
use dioxus::desktop::use_muda_event_handler;
use dioxus::prelude::*;

use crate::document::Document;
use crate::settings::Settings;

/// Context menu item ids.
const MENU_LINE_NUMBERS: &str = "rmd.editor.line-numbers";

#[component]
pub fn Editor(doc: Signal<Document>) -> Element {
    let mut settings = use_context::<Signal<Settings>>();
    let line_numbers = settings.read().line_numbers;

    use_muda_event_handler(move |event: &MenuEvent| {
        if event.id == MENU_LINE_NUMBERS {
            let shown = settings.read().line_numbers;
            settings.write().line_numbers = !shown;
        }
    });

    rsx! {
        div {
            class: "editor-wrap",
            oncontextmenu: move |event| {
                event.prevent_default();
                // Shown after the event returns: the native menu runs a modal
                // loop that must not block the web view's event dispatch.
                spawn(async move { show_context_menu(line_numbers) });
            },
            if line_numbers {
                // Filled and scrolled by editor.js, which measures wrapped lines.
                div { class: "gutter", "data-rmd-generated": "true",
                    div { class: "gutter-lines" }
                }
            }
            textarea {
                class: "editor",
                spellcheck: "false",
                placeholder: "Écrivez du Markdown ici…",
                value: "{doc.read().content}",
                oninput: move |event| doc.write().content = event.value(),
            }
        }
    }
}

/// Native Windows context menu: keeps the usual clipboard commands next to ours.
fn show_context_menu(line_numbers: bool) {
    let menu = Menu::new();
    let items: [&dyn dioxus::desktop::muda::IsMenuItem; 7] = [
        &PredefinedMenuItem::cut(Some("Couper")),
        &PredefinedMenuItem::copy(Some("Copier")),
        &PredefinedMenuItem::paste(Some("Coller")),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::select_all(Some("Tout sélectionner")),
        &PredefinedMenuItem::separator(),
        &CheckMenuItem::with_id(
            MENU_LINE_NUMBERS,
            "Numéros de ligne",
            true,
            line_numbers,
            None,
        ),
    ];
    if menu.append_items(&items).is_err() {
        return;
    }
    popup(&menu);
}

#[allow(unsafe_code)]
fn popup(menu: &Menu) {
    use dioxus::desktop::tao::platform::windows::WindowExtWindows;
    let hwnd = dioxus::desktop::window().window.hwnd();
    // SAFETY: `hwnd` is the handle of this application's live main window,
    // obtained from tao on the UI thread; muda only requires a valid HWND.
    // `None` positions the menu at the mouse cursor.
    unsafe {
        menu.show_context_menu_for_hwnd(hwnd, None);
    }
}
