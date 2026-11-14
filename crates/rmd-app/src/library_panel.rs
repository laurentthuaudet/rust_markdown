//! Sidebar listing environments and their documents, with filters.

use dioxus::prelude::*;

use crate::icons::{Glyph, Icon};
use rfd::{AsyncFileDialog, AsyncMessageDialog, MessageButtons, MessageDialogResult, MessageLevel};
use rmd_store::{EnvironmentId, Filter, MATCH_END, MATCH_START, SearchHit};

use crate::app::open_path;
use crate::document::Document;
use crate::library::{Library, day_end, day_start, format_time};
use crate::notice::{Notice, Status};

/// Splits a search snippet into (text, is_match) segments.
fn snippet_parts(snippet: &str) -> Vec<(String, bool)> {
    let mut parts = Vec::new();
    for (i, chunk) in snippet.split(MATCH_START).enumerate() {
        match chunk.split_once(MATCH_END) {
            Some((matched, rest)) if i > 0 => {
                parts.push((matched.to_owned(), true));
                parts.push((rest.to_owned(), false));
            }
            _ => parts.push((chunk.to_owned(), false)),
        }
    }
    parts.retain(|(text, _)| !text.is_empty());
    parts
}

#[component]
pub fn LibraryPanel(doc: Signal<Document>, status: Status) -> Element {
    let library = use_context::<Library>();
    let mut selected = use_signal(|| None::<EnvironmentId>);
    let mut name = use_signal(String::new);
    let mut text = use_signal(String::new);
    let mut tags = use_signal(Vec::<String>::new);
    let mut from = use_signal(String::new);
    let mut to = use_signal(String::new);
    // Draft name while creating / renaming an environment.
    let mut creating = use_signal(|| None::<String>);
    let mut renaming = use_signal(|| None::<String>);

    let lib = library.clone();
    let environments = use_memo(move || lib.read(|store| store.environments()).unwrap_or_default());
    let lib = library.clone();
    let available_tags =
        use_memo(move || lib.read(|store| store.tags(selected())).unwrap_or_default());
    let lib = library.clone();
    let hits = use_memo(move || {
        let filter = Filter {
            environment: selected(),
            name: name(),
            tags: tags(),
            modified_from: day_start(&from()),
            modified_to: day_end(&to()),
            text: text(),
        };
        lib.read(|store| store.search(&filter)).unwrap_or_default()
    });

    if let Some(error) = library.error() {
        return rsx! {
            aside { class: "library",
                p { class: "muted", "Bibliothèque indisponible : {error}" }
            }
        };
    }

    let current = selected().and_then(|id| environments().into_iter().find(|env| env.id == id));
    // Each closure below needs its own copy of the selected environment name.
    let rename_seed = current
        .as_ref()
        .map(|env| env.name.clone())
        .unwrap_or_default();
    let delete_name = rename_seed.clone();
    let filtering = !name().is_empty()
        || !text().is_empty()
        || !tags().is_empty()
        || !from().is_empty()
        || !to().is_empty();

    let mut create = move || {
        let Some(draft) = creating() else { return };
        let library = consume_context::<Library>();
        match library.write(|store| store.create_environment(&draft, "")) {
            Ok(env) => {
                creating.set(None);
                selected.set(Some(env.id));
            }
            Err(err) => status.set(Some(Notice::error(err))),
        }
    };
    let mut rename = move || {
        let (Some(draft), Some(id)) = (renaming(), selected()) else {
            return;
        };
        let library = consume_context::<Library>();
        match library.write(|store| store.update_environment(id, &draft, "")) {
            Ok(_) => renaming.set(None),
            Err(err) => status.set(Some(Notice::error(err))),
        }
    };

    rsx! {
        aside { class: "library",
            div { class: "section-title",
                span { "Environnements" }
                div {
                    button {
                        class: "icon",
                        title: "Réindexer les fichiers modifiés",
                        onclick: move |_| refresh(status),
                        Glyph { icon: Icon::Refresh }
                    }
                    button {
                        class: "icon",
                        title: "Nouvel environnement",
                        onclick: move |_| creating.set(Some(String::new())),
                        Glyph { icon: Icon::Plus }
                    }
                }
            }
            ul { class: "env-list",
                li {
                    class: if selected().is_none() { "active" },
                    onclick: move |_| { selected.set(None); renaming.set(None); },
                    span { "Tous les documents" }
                }
                for env in environments() {
                    li {
                        key: "{env.id.0}",
                        class: if selected() == Some(env.id) { "active" },
                        onclick: move |_| { selected.set(Some(env.id)); renaming.set(None); },
                        span { "{env.name}" }
                        span { class: "count", "{env.document_count}" }
                    }
                }
            }
            if let Some(draft) = creating() {
                input {
                    class: "inline-input",
                    autofocus: true,
                    placeholder: "Nom du nouvel environnement",
                    value: "{draft}",
                    oninput: move |event| creating.set(Some(event.value())),
                    onkeydown: move |event| match event.key() {
                        Key::Enter => create(),
                        Key::Escape => creating.set(None),
                        _ => {}
                    },
                    onblur: move |_| {
                        if creating().is_some_and(|draft| draft.trim().is_empty()) {
                            creating.set(None);
                        }
                    },
                }
            }
            if let Some(env) = current {
                div { class: "env-actions",
                    if let Some(draft) = renaming() {
                        input {
                            class: "inline-input",
                            autofocus: true,
                            value: "{draft}",
                            oninput: move |event| renaming.set(Some(event.value())),
                            onkeydown: move |event| match event.key() {
                                Key::Enter => rename(),
                                Key::Escape => renaming.set(None),
                                _ => {}
                            },
                        }
                    } else {
                        button { onclick: move |_| { spawn(add_files(env.id, status)); }, "Ajouter des fichiers" }
                        button { onclick: move |_| { spawn(add_folder(env.id, status)); }, "Ajouter un dossier" }
                        button { onclick: move |_| renaming.set(Some(rename_seed.clone())), "Renommer" }
                        button {
                            class: "danger",
                            onclick: move |_| { spawn(delete_environment(env.id, delete_name.clone(), selected, status)); },
                            "Supprimer"
                        }
                    }
                }
            }

            div { class: "section-title", span { "Filtres" }
                if filtering {
                    button {
                        class: "link",
                        onclick: move |_| {
                            name.set(String::new());
                            text.set(String::new());
                            tags.set(Vec::new());
                            from.set(String::new());
                            to.set(String::new());
                        },
                        "Réinitialiser"
                    }
                }
            }
            div { class: "filters",
                input {
                    r#type: "search",
                    placeholder: "Rechercher dans le contenu…",
                    value: "{text}",
                    oninput: move |event| text.set(event.value()),
                }
                input {
                    r#type: "search",
                    placeholder: "Nom ou titre…",
                    value: "{name}",
                    oninput: move |event| name.set(event.value()),
                }
                div { class: "dates",
                    label { "Modifié du" }
                    input { r#type: "date", value: "{from}", oninput: move |event| from.set(event.value()) }
                    label { "au" }
                    input { r#type: "date", value: "{to}", oninput: move |event| to.set(event.value()) }
                }
                if !available_tags().is_empty() {
                    div { class: "tag-cloud",
                        for tag in available_tags() {
                            button {
                                key: "{tag.name}",
                                class: if tags.read().contains(&tag.name) { "tag active" } else { "tag" },
                                onclick: move |_| {
                                    let mut tags = tags.write();
                                    match tags.iter().position(|t| *t == tag.name) {
                                        Some(index) => { tags.remove(index); }
                                        None => tags.push(tag.name.clone()),
                                    }
                                },
                                "#{tag.name} "
                                span { class: "count", "{tag.count}" }
                            }
                        }
                    }
                }
            }

            ul { class: "results",
                for hit in hits() {
                    ResultItem { key: "{hit.document.id.0}", hit, environment: selected(), doc, status }
                }
            }
            if hits().is_empty() {
                p { class: "muted",
                    if environments().is_empty() {
                        "Créez un environnement avec « + » puis ajoutez-y des fichiers Markdown."
                    } else {
                        "Aucun document."
                    }
                }
            }
        }
    }
}

#[component]
fn ResultItem(
    hit: SearchHit,
    environment: Option<EnvironmentId>,
    doc: Signal<Document>,
    status: Status,
) -> Element {
    let document = hit.document.clone();
    let is_open = doc.read().path() == Some(document.path.as_path());
    let mut class = String::from("result");
    if is_open {
        class.push_str(" open");
    }
    if document.missing {
        class.push_str(" missing");
    }
    let path = document.path.clone();
    let id = document.id;
    rsx! {
        li {
            class,
            title: "{document.path.display()}",
            onclick: move |_| {
                if !document.missing {
                    spawn(open_path(doc, status, path.clone()));
                }
            },
            div { class: "result-title",
                span { "{hit.document.title}" }
                if let Some(env) = environment {
                    button {
                        class: "icon remove",
                        title: "Retirer de l'environnement",
                        onclick: move |event| {
                            event.stop_propagation();
                            let library = consume_context::<Library>();
                            if let Err(err) = library.write(|store| store.remove_document(env, id)) {
                                status.set(Some(Notice::error(err)));
                            }
                        },
                        Glyph { icon: Icon::Close, size: 14 }
                    }
                }
            }
            div { class: "result-meta",
                "{hit.document.file_name}"
                if let Some(modified) = hit.document.modified_at { " · {format_time(modified)}" }
                if hit.document.missing { " · introuvable" }
            }
            if !hit.tags.is_empty() {
                div { class: "result-tags",
                    for tag in hit.tags.iter() { span { class: "tag", "#{tag}" } }
                }
            }
            if let Some(snippet) = hit.snippet.as_deref() {
                p { class: "snippet",
                    for (part, matched) in snippet_parts(snippet) {
                        if matched { mark { "{part}" } } else { "{part}" }
                    }
                }
            }
        }
    }
}

fn refresh(mut status: Status) {
    let library = consume_context::<Library>();
    match library.write(|store| store.refresh()) {
        Ok(report) => status.set(Some(Notice::info(format!(
            "Index à jour : {} fichier(s) réindexé(s), {} introuvable(s).",
            report.reindexed, report.missing
        )))),
        Err(err) => status.set(Some(Notice::error(err))),
    }
}

async fn add_files(env: EnvironmentId, mut status: Status) {
    let Some(files) = AsyncFileDialog::new()
        .set_title("Ajouter des fichiers Markdown")
        .add_filter("Markdown", &["md", "markdown", "mdown", "mkd"])
        .pick_files()
        .await
    else {
        return;
    };
    let library = consume_context::<Library>();
    let result = library.write(|store| {
        for file in &files {
            store.add_document(env, file.path())?;
        }
        Ok(files.len())
    });
    status.set(Some(match result {
        Ok(count) => Notice::info(format!("{count} fichier(s) ajouté(s).")),
        Err(err) => Notice::error(err),
    }));
}

async fn add_folder(env: EnvironmentId, mut status: Status) {
    let Some(folder) = AsyncFileDialog::new()
        .set_title("Ajouter tous les fichiers Markdown d'un dossier")
        .pick_folder()
        .await
    else {
        return;
    };
    let library = consume_context::<Library>();
    status.set(Some(
        match library.write(|store| store.add_folder(env, folder.path())) {
            Ok(count) => Notice::info(format!("{count} fichier(s) ajouté(s) depuis le dossier.")),
            Err(err) => Notice::error(err),
        },
    ));
}

async fn delete_environment(
    env: EnvironmentId,
    name: String,
    mut selected: Signal<Option<EnvironmentId>>,
    mut status: Status,
) {
    let answer = AsyncMessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title("Supprimer l'environnement")
        .set_description(format!(
            "Supprimer l'environnement « {name} » ?\n\nLes fichiers eux-mêmes ne sont pas supprimés."
        ))
        .set_buttons(MessageButtons::YesNo)
        .show()
        .await;
    if answer != MessageDialogResult::Yes {
        return;
    }
    let library = consume_context::<Library>();
    match library.write(|store| store.delete_environment(env)) {
        Ok(()) => selected.set(None),
        Err(err) => status.set(Some(Notice::error(err))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_snippets() {
        let snippet = format!("a {MATCH_START}b{MATCH_END} c {MATCH_START}d{MATCH_END}");
        assert_eq!(
            snippet_parts(&snippet),
            [
                ("a ".to_owned(), false),
                ("b".to_owned(), true),
                (" c ".to_owned(), false),
                ("d".to_owned(), true),
            ]
        );
        assert_eq!(snippet_parts("plain"), [("plain".to_owned(), false)]);
    }
}
