//! Export of Markdown documents to HTML, PDF, DOCX and EPUB.
//!
//! Every exporter is pure Rust: no external tool (pandoc, LaTeX, browser) is
//! needed at runtime.

mod docx;
mod epub;
mod html;
mod pdf;
mod resources;
mod typst_markup;

use std::fmt;
use std::path::{Path, PathBuf};

pub use resources::LocalImage;

/// Output formats supported by [`export`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    Html,
    Pdf,
    Docx,
    Epub,
}

impl Format {
    pub const ALL: &[Format] = &[Format::Html, Format::Pdf, Format::Docx, Format::Epub];

    /// File extension, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Html => "html",
            Format::Pdf => "pdf",
            Format::Docx => "docx",
            Format::Epub => "epub",
        }
    }

    /// Human-readable name.
    pub fn label(self) -> &'static str {
        match self {
            Format::Html => "HTML",
            Format::Pdf => "PDF",
            Format::Docx => "Word (DOCX)",
            Format::Epub => "EPUB",
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// A document to export.
#[derive(Debug, Clone)]
pub struct Document<'a> {
    /// Markdown source.
    pub markdown: &'a str,
    /// Title used in metadata (HTML `<title>`, EPUB/DOCX properties…).
    pub title: String,
    /// Directory against which relative image paths are resolved.
    pub base_dir: Option<PathBuf>,
}

impl<'a> Document<'a> {
    /// Builds a document, deriving its title from the first heading or the file name.
    pub fn new(markdown: &'a str, path: Option<&Path>) -> Self {
        let title = rmd_core::title(markdown)
            .or_else(|| {
                path.and_then(Path::file_stem)
                    .map(|stem| stem.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "Document".to_owned());
        Self {
            markdown,
            title,
            base_dir: path.and_then(Path::parent).map(Path::to_path_buf),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{format} export failed: {message}")]
    Backend { format: Format, message: String },
}

impl ExportError {
    pub(crate) fn backend(format: Format, message: impl fmt::Display) -> Self {
        Self::Backend {
            format,
            message: message.to_string(),
        }
    }
}

/// Converts `doc` to `format` and returns the file contents.
pub fn export(doc: &Document<'_>, format: Format) -> Result<Vec<u8>, ExportError> {
    match format {
        Format::Html => Ok(html::export(doc).into_bytes()),
        Format::Pdf => pdf::export(doc),
        Format::Docx => docx::export(doc),
        Format::Epub => epub::export(doc),
    }
}

/// Converts `doc` to `format` and writes it to `path`.
pub fn export_to_file(
    doc: &Document<'_>,
    format: Format,
    path: impl AsRef<Path>,
) -> Result<(), ExportError> {
    let bytes = export(doc, format)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_title() {
        let doc = Document::new("# Hello\n\ntext", Some(Path::new(r"C:\notes\file.md")));
        assert_eq!(doc.title, "Hello");
        assert_eq!(doc.base_dir.as_deref(), Some(Path::new(r"C:\notes")));

        let doc = Document::new("no heading", Some(Path::new(r"C:\notes\file.md")));
        assert_eq!(doc.title, "file");
    }
}
