//! Bar above the editor: environments and tags of the open document.

use std::path::Path;

use dioxus::prelude::*;
use rmd_store::EnvironmentId;

use crate::document::Document;
use crate::library::Library;
use crate::notice::{Notice, Status};

#[component]
pub fn DocumentBar(doc: Signal<Document>, status: Status) -> Element {
    let library = use_context::<Library>();
    let path = use_memo(move || doc.read().path().map(Path::to_path_buf));

    let lib = library.clone();
    let entry = use_memo(move || {
        let path = path()?;
        lib.read(|store| store.document_by_path(&path)).flatten()
    });
    let lib = library.clone();
    let memberships = use_memo(move || match entry() {
        Some(entry) => lib
            .read(|store| store.environments_of(entry.id))
            .unwrap_or_default(),
        None => Vec::new(),
    });
    let lib = library.clone();
    let tags = use_memo(move || match entry() {
        Some(entry) => lib
            .read(|store| store.tags_of(entry.id))
            .unwrap_or_default(),
        None => Vec::new(),
    });
    let lib = library.clone();
    let environments = use_memo(move || lib.read(|store| store.environments()).unwrap_or_default());

    if library.error().is_some() || path().is_none() {
        return rsx! {};
    }
    let (member, other): (Vec<_>, Vec<_>) = environments()
        .into_iter()
        .partition(|env| memberships().contains(&env.id));

    rsx! {
        div { class: "docbar",
            span { class: "label", "Environnements" }
            for env in member {
                span { key: "{env.id.0}", class: "chip",
                    "{env.name}"
                    button {
                        class: "link",
                        title: "Retirer de cet environnement",
                        onclick: move |_| {
                            let Some(entry) = entry() else { return };
                            let library = consume_context::<Library>();
                            if let Err(err) = library.write(|store| store.remove_document(env.id, entry.id)) {
                                status.set(Some(Notice::error(err)));
                            }
                        },
                        "✕"
                    }
                }
            }
            if !other.is_empty() {
                select {
                    value: "",
                    onchange: move |event| {
                        let Ok(id) = event.value().parse::<i64>() else { return };
                        let Some(path) = path() else { return };
                        let library = consume_context::<Library>();
                        if let Err(err) = library.write(|store| store.add_document(EnvironmentId(id), &path)) {
                            status.set(Some(Notice::error(err)));
                        }
                    },
                    option { value: "", "Ajouter à…" }
                    for env in other {
                        option { key: "{env.id.0}", value: "{env.id.0}", "{env.name}" }
                    }
                }
            }
            span { class: "label", "Tags" }
            if let Some(entry) = entry() {
                input {
                    class: "tags-input",
                    placeholder: "projet, brouillon…",
                    title: "Tags séparés par des virgules (Entrée pour valider)",
                    value: "{tags().join(\", \")}",
                    onchange: move |event| {
                        let value = event.value();
                        let new_tags: Vec<&str> = value.split(',').collect();
                        let library = consume_context::<Library>();
                        if let Err(err) = library.write(|store| store.set_tags(entry.id, &new_tags)) {
                            status.set(Some(Notice::error(err)));
                        }
                    },
                }
            } else {
                span { class: "muted", "ajoutez le document à un environnement pour le taguer" }
            }
        }
    }
}
