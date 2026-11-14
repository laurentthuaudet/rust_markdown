//! "Export" toolbar menu.

use std::path::{Path, PathBuf};

use dioxus::prelude::*;

use crate::icons::{Glyph, Icon};
use rfd::AsyncFileDialog;
use rmd_export::Format;

use crate::document::Document;
use crate::notice::{Notice, Status};

#[component]
pub fn ExportMenu(doc: Signal<Document>, status: Status) -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        div { class: "menu",
            // Closing when the pointer leaves keeps the menu simple and predictable.
            onmouseleave: move |_| open.set(false),
            button {
                title: "Exporter le document",
                onclick: move |_| open.toggle(),
                "Exporter"
                Glyph { icon: Icon::ChevronDown, size: 14 }
            }
            if open() {
                div { class: "menu-list",
                    for &format in Format::ALL {
                        button {
                            onclick: move |_| {
                                open.set(false);
                                spawn(export(doc, status, format));
                            },
                            "{format.label()}"
                        }
                    }
                }
            }
        }
    }
}

/// Asks for a destination, then exports on a worker thread (PDF can take a
/// moment the first time, while fonts load).
async fn export(doc: Signal<Document>, mut status: Status, format: Format) {
    let (markdown, source, stem) = {
        let doc = doc.read();
        let stem = doc
            .path()
            .and_then(Path::file_stem)
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "document".to_owned());
        (doc.content.clone(), doc.path().map(Path::to_path_buf), stem)
    };

    let mut dialog = AsyncFileDialog::new()
        .set_title(format!("Exporter en {}", format.label()))
        .add_filter(format.label(), &[format.extension()])
        .set_file_name(format!("{stem}.{}", format.extension()));
    if let Some(dir) = source.as_deref().and_then(Path::parent) {
        dialog = dialog.set_directory(dir);
    }
    let Some(target) = dialog.save_file().await else {
        return;
    };
    let target = with_extension(target.path(), format);

    status.set(Some(Notice::info(format!(
        "Export {} en cours…",
        format.label()
    ))));
    let destination = target.clone();
    let result = tokio::task::spawn_blocking(move || {
        let document = rmd_export::Document::new(&markdown, source.as_deref());
        rmd_export::export_to_file(&document, format, &destination)
    })
    .await;

    status.set(Some(match result {
        Ok(Ok(())) => Notice::info(format!("Exporté vers {}", target.display())),
        Ok(Err(err)) => Notice::error(format!("Échec de l'export : {err}")),
        Err(err) => Notice::error(format!("Échec de l'export : {err}")),
    }));
}

fn with_extension(path: &Path, format: Format) -> PathBuf {
    let matches = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case(format.extension()));
    if matches {
        path.to_path_buf()
    } else {
        let mut name = path.as_os_str().to_owned();
        name.push(".");
        name.push(format.extension());
        PathBuf::from(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_missing_extension() {
        assert_eq!(
            with_extension(Path::new(r"C:\out\doc"), Format::Pdf),
            Path::new(r"C:\out\doc.pdf")
        );
        assert_eq!(
            with_extension(Path::new(r"C:\out\doc.PDF"), Format::Pdf),
            Path::new(r"C:\out\doc.PDF")
        );
        assert_eq!(
            with_extension(Path::new(r"C:\out\v1.2"), Format::Docx),
            Path::new(r"C:\out\v1.2.docx")
        );
    }
}
