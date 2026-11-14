//! Find & replace bar above the editor (Ctrl+F / Ctrl+H).

use dioxus::prelude::*;

use crate::icons::{Glyph, Icon};

use crate::document::Document;
use crate::find::{find_all, replace_all, replace_one, to_utf16};
use crate::notice::{Notice, Status};

/// Find bar state, shared so that shortcuts and menus can open it.
#[derive(Clone, Copy)]
pub struct FindState {
    pub open: Signal<bool>,
    pub replace: Signal<bool>,
    pub query: Signal<String>,
    pub replacement: Signal<String>,
    pub case_sensitive: Signal<bool>,
    /// Index of the current match.
    pub current: Signal<usize>,
    /// Bumped to (re)focus the search field.
    focus: Signal<u32>,
}

impl FindState {
    pub fn new() -> Self {
        Self {
            open: Signal::new(false),
            replace: Signal::new(false),
            query: Signal::new(String::new()),
            replacement: Signal::new(String::new()),
            case_sensitive: Signal::new(false),
            current: Signal::new(0),
            focus: Signal::new(0),
        }
    }

    /// Opens the bar (with the replace row if asked) and focuses the search field.
    pub fn show(mut self, with_replace: bool) {
        self.open.set(true);
        if with_replace {
            self.replace.set(true);
        }
        self.focus += 1;
    }
}

/// JS call updating the match highlights in the editor.
fn highlight_script(ranges: &[(usize, usize)], current: Option<usize>) -> String {
    let ranges = serde_json::to_string(ranges).unwrap_or_else(|_| "[]".into());
    let current = current.map_or(-1, |index| index as i64);
    format!("window.rmdEditor && window.rmdEditor.highlight({ranges}, {current});")
}

#[component]
pub fn FindBar(doc: Signal<Document>, status: Status) -> Element {
    let state = use_context::<FindState>();
    let FindState {
        mut open,
        mut replace,
        mut query,
        mut replacement,
        mut case_sensitive,
        mut current,
        focus,
    } = state;

    let matches = use_memo(move || find_all(&doc.read().content, &query(), case_sensitive()));
    let total = matches.read().len();
    let index = if total == 0 {
        None
    } else {
        Some(current() % total)
    };

    // Highlight every match and scroll to the current one.
    use_effect(move || {
        let matches = matches.read();
        let script = if open() {
            let total = matches.len();
            let ranges = to_utf16(&doc.read().content, &matches);
            let index = (total > 0).then(|| current() % total);
            highlight_script(&ranges, index)
        } else {
            highlight_script(&[], None)
        };
        let _ = document::eval(&script);
    });

    // Focus the search field whenever the bar is (re)opened.
    use_effect(move || {
        focus();
        let _ = document::eval(
            "setTimeout(() => { const input = document.querySelector('.find-bar input'); \
             if (input) { input.focus(); input.select(); } }, 0);",
        );
    });

    let mut step = move |delta: isize| {
        let total = matches.read().len();
        if total > 0 {
            let next = (current() % total) as isize + delta;
            current.set(next.rem_euclid(total as isize) as usize);
        }
    };

    let mut close = move || {
        open.set(false);
        // Leave the current match selected in the editor.
        let total = matches.read().len();
        let selection = (total > 0).then(|| {
            let ranges = to_utf16(&doc.read().content, &matches.read());
            ranges[current() % total]
        });
        let script = match selection {
            Some((start, end)) => {
                format!("window.rmdEditor && window.rmdEditor.select({start}, {end});")
            }
            None => "document.querySelector('textarea.editor')?.focus();".to_owned(),
        };
        let _ = document::eval(&script);
    };

    let on_keydown = move |event: KeyboardEvent| match event.key() {
        Key::Enter | Key::F3 => {
            event.prevent_default();
            step(if event.modifiers().shift() { -1 } else { 1 });
        }
        Key::Escape => {
            event.prevent_default();
            close();
        }
        _ => {}
    };

    let mut replace_current = move || {
        let Some(index) = index else { return };
        let range = matches.read()[index].clone();
        let text = replace_one(&doc.read().content, &range, &replacement());
        doc.write().content = text;
        // The next match now has the same index.
    };
    let mut replace_every = move || {
        let (text, count) = replace_all(
            &doc.read().content,
            &query(),
            &replacement(),
            case_sensitive(),
        );
        if count > 0 {
            doc.write().content = text;
        }
        status.set(Some(Notice::info(format!(
            "{count} remplacement(s) effectué(s)."
        ))));
    };

    if !open() {
        return rsx! {};
    }

    let counter = match index {
        Some(index) => format!("{} / {total}", index + 1),
        None if query.read().is_empty() => String::new(),
        None => "Aucun résultat".to_owned(),
    };

    rsx! {
        div { class: "find-bar", onkeydown: on_keydown,
            div { class: "find-row",
                button {
                    class: "icon",
                    title: if replace() { "Masquer le remplacement" } else { "Afficher le remplacement (Ctrl+H)" },
                    onclick: move |_| replace.toggle(),
                    Glyph { icon: if replace() { Icon::ChevronDown } else { Icon::ChevronRight } }
                }
                input {
                    r#type: "text",
                    placeholder: "Rechercher",
                    value: "{query}",
                    oninput: move |event| {
                        query.set(event.value());
                        current.set(0);
                    },
                }
                button {
                    class: if case_sensitive() { "icon toggle active" } else { "icon toggle" },
                    title: "Respecter la casse",
                    onclick: move |_| case_sensitive.toggle(),
                    "Aa"
                }
                span { class: "find-count", "{counter}" }
                button { class: "icon", title: "Précédent (Maj+Entrée)", disabled: total == 0, onclick: move |_| step(-1), Glyph { icon: Icon::ArrowUp } }
                button { class: "icon", title: "Suivant (Entrée)", disabled: total == 0, onclick: move |_| step(1), Glyph { icon: Icon::ArrowDown } }
                button { class: "icon", title: "Fermer (Échap)", onclick: move |_| close(), Glyph { icon: Icon::Close } }
            }
            if replace() {
                div { class: "find-row",
                    span { class: "find-indent" }
                    input {
                        r#type: "text",
                        placeholder: "Remplacer par",
                        value: "{replacement}",
                        oninput: move |event| replacement.set(event.value()),
                    }
                    button { disabled: total == 0, onclick: move |_| replace_current(), "Remplacer" }
                    button { disabled: total == 0, onclick: move |_| replace_every(), "Tout remplacer" }
                }
            }
        }
    }
}
