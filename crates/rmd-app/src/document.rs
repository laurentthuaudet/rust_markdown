//! The Markdown document currently open in the editor.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// A document being viewed/edited, with its on-disk location (if any).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    path: Option<PathBuf>,
    /// Current text, as shown in the editor.
    pub content: String,
    /// Text as last read from or written to disk, to detect unsaved changes.
    saved: String,
    /// The file uses Windows line endings. The editor (a textarea) only knows
    /// `\n`, so the content is kept with `\n` and converted back on save.
    crlf: bool,
}

impl Document {
    /// A new, empty, never-saved document.
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads a document from disk.
    pub fn open(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        let raw = fs::read_to_string(&path)?;
        let crlf = raw.contains("\r\n");
        let content = raw.replace("\r\n", "\n");
        Ok(Self {
            path: Some(path),
            saved: content.clone(),
            content,
            crlf,
        })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Directory used to resolve relative links and images.
    pub fn base_dir(&self) -> Option<PathBuf> {
        self.path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
    }

    /// File name shown in the title bar.
    pub fn display_name(&self) -> String {
        self.path
            .as_deref()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Sans titre".to_owned())
    }

    pub fn is_dirty(&self) -> bool {
        self.content != self.saved
    }

    /// Writes the document to its current path.
    ///
    /// Fails with [`io::ErrorKind::NotFound`] if the document has never been saved.
    pub fn save(&mut self) -> io::Result<()> {
        let path = self.path.clone().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "document has no file path yet")
        })?;
        self.save_as(path)
    }

    /// Writes the document to `path`, which becomes its new location.
    pub fn save_as(&mut self, path: impl Into<PathBuf>) -> io::Result<()> {
        let path = path.into();
        if self.crlf {
            fs::write(&path, self.content.replace('\n', "\r\n"))?;
        } else {
            fs::write(&path, &self.content)?;
        }
        self.saved = self.content.clone();
        self.path = Some(path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_unsaved_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        fs::write(&path, "# Hi").unwrap();

        let mut doc = Document::open(&path).unwrap();
        assert!(!doc.is_dirty());
        assert_eq!(doc.display_name(), "note.md");
        assert_eq!(doc.base_dir().as_deref(), Some(dir.path()));

        doc.content.push_str("\n\nmore");
        assert!(doc.is_dirty());
        doc.save().unwrap();
        assert!(!doc.is_dirty());
        assert_eq!(fs::read_to_string(&path).unwrap(), "# Hi\n\nmore");
    }

    #[test]
    fn new_document_needs_a_path_to_save() {
        let mut doc = Document::new();
        doc.content = "text".into();
        assert_eq!(doc.display_name(), "Sans titre");
        assert_eq!(doc.save().unwrap_err().kind(), io::ErrorKind::NotFound);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.md");
        doc.save_as(&path).unwrap();
        assert_eq!(doc.path(), Some(path.as_path()));
        assert!(!doc.is_dirty());
    }
}

#[cfg(test)]
mod line_ending_tests {
    use super::*;

    #[test]
    fn keeps_windows_line_endings_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crlf.md");
        fs::write(&path, "# A\r\n\r\ntext\r\n").unwrap();

        let mut doc = Document::open(&path).unwrap();
        assert_eq!(doc.content, "# A\n\ntext\n");
        assert!(!doc.is_dirty());

        doc.content.push_str("more\n");
        doc.save().unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# A\r\n\r\ntext\r\nmore\r\n"
        );
    }

    #[test]
    fn leaves_unix_line_endings_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lf.md");
        fs::write(&path, "a\nb\n").unwrap();
        let mut doc = Document::open(&path).unwrap();
        doc.save().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "a\nb\n");
    }
}
