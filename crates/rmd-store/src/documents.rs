//! Documents referenced by environments, and their index entries.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rusqlite::{OptionalExtension, params};

use crate::{EnvironmentId, Result, Store, StoreError, now};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocumentId(pub i64);

/// A document as stored in the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentInfo {
    pub id: DocumentId,
    pub path: PathBuf,
    pub file_name: String,
    /// First heading of the document, or its file name.
    pub title: String,
    /// File modification time (Unix seconds) at the last indexing.
    pub modified_at: Option<i64>,
    /// The file could not be found at the last refresh.
    pub missing: bool,
}

/// Outcome of [`Store::refresh`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RefreshReport {
    pub reindexed: usize,
    pub missing: usize,
}

/// File extensions picked up when adding a folder.
const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown", "mdown", "mkd"];
/// Folders skipped when adding a folder recursively.
const SKIPPED_DIRS: &[&str] = &["node_modules", "target"];

pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            MARKDOWN_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

fn modified_at(path: &Path) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(modified.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64)
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

impl Store {
    /// Adds a Markdown file to an environment and indexes it.
    ///
    /// Adding a file already in the environment is a no-op (besides reindexing).
    pub fn add_document(&self, env: EnvironmentId, path: &Path) -> Result<DocumentId> {
        self.environment(env)?;
        let path = std::path::absolute(path)?;
        if !path.is_file() {
            return Err(StoreError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("{} is not a file", path.display()),
            )));
        }
        let id = self.upsert_document(&path)?;
        self.conn.execute(
            "INSERT OR IGNORE INTO environment_documents (environment_id, document_id, added_at)
             VALUES (?1, ?2, ?3)",
            params![env.0, id.0, now()],
        )?;
        Ok(id)
    }

    /// Adds every Markdown file below `dir` (recursively) to an environment.
    ///
    /// Hidden folders (`.git`…) and build folders are skipped. Returns the
    /// number of files added.
    pub fn add_folder(&self, env: EnvironmentId, dir: &Path) -> Result<usize> {
        let mut files = Vec::new();
        collect_markdown_files(dir, &mut files)?;
        files.sort();
        let tx = self.conn.unchecked_transaction()?;
        for file in &files {
            self.add_document(env, file)?;
        }
        tx.commit()?;
        Ok(files.len())
    }

    /// Removes a document from an environment (the file itself is untouched).
    pub fn remove_document(&self, env: EnvironmentId, doc: DocumentId) -> Result<()> {
        self.conn.execute(
            "DELETE FROM environment_documents WHERE environment_id = ?1 AND document_id = ?2",
            params![env.0, doc.0],
        )?;
        self.remove_orphan_documents()
    }

    pub fn document(&self, id: DocumentId) -> Result<DocumentInfo> {
        self.conn
            .query_row(
                &format!("{SELECT_DOCUMENT} WHERE d.id = ?1"),
                [id.0],
                row_to_document,
            )
            .optional()?
            .ok_or(StoreError::NotFound)
    }

    /// Looks a document up by path.
    pub fn document_by_path(&self, path: &Path) -> Result<Option<DocumentInfo>> {
        let path = std::path::absolute(path)?;
        Ok(self
            .conn
            .query_row(
                &format!("{SELECT_DOCUMENT} WHERE d.path = ?1"),
                [path_to_string(&path)],
                row_to_document,
            )
            .optional()?)
    }

    /// Environments a document belongs to.
    pub fn environments_of(&self, doc: DocumentId) -> Result<Vec<EnvironmentId>> {
        let mut stmt = self.conn.prepare(
            "SELECT environment_id FROM environment_documents WHERE document_id = ?1
             ORDER BY environment_id",
        )?;
        let rows = stmt.query_map([doc.0], |row| row.get(0).map(EnvironmentId))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Reindexes documents whose file changed on disk and flags missing ones.
    pub fn refresh(&self) -> Result<RefreshReport> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, path, modified_at, missing FROM documents")?;
        let rows: Vec<(i64, String, Option<i64>, bool)> = stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .collect::<rusqlite::Result<_>>()?;

        let mut report = RefreshReport::default();
        let tx = self.conn.unchecked_transaction()?;
        for (id, path, indexed_mtime, was_missing) in rows {
            let path = PathBuf::from(path);
            match modified_at(&path) {
                None => {
                    report.missing += 1;
                    if !was_missing {
                        self.conn
                            .execute("UPDATE documents SET missing = 1 WHERE id = ?1", [id])?;
                    }
                }
                Some(mtime) if was_missing || Some(mtime) != indexed_mtime => {
                    self.index(DocumentId(id), &path)?;
                    report.reindexed += 1;
                }
                Some(_) => {}
            }
        }
        tx.commit()?;
        Ok(report)
    }

    /// Reindexes a single document now (e.g. right after saving it).
    pub fn reindex(&self, id: DocumentId) -> Result<()> {
        let doc = self.document(id)?;
        self.index(id, &doc.path)
    }

    /// Inserts or refreshes the document row for an absolute path.
    fn upsert_document(&self, path: &Path) -> Result<DocumentId> {
        let existing: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM documents WHERE path = ?1",
                [path_to_string(path)],
                |row| row.get(0),
            )
            .optional()?;
        let id = match existing {
            Some(id) => DocumentId(id),
            None => {
                let file_name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.conn.execute(
                    "INSERT INTO documents (path, file_name, indexed_at) VALUES (?1, ?2, ?3)",
                    params![path_to_string(path), file_name, now()],
                )?;
                DocumentId(self.conn.last_insert_rowid())
            }
        };
        self.index(id, path)?;
        Ok(id)
    }

    /// Reads the file and updates its title, date and full-text entry.
    fn index(&self, id: DocumentId, path: &Path) -> Result<()> {
        // Tolerate files that are not valid UTF-8 rather than failing a whole folder.
        let content = String::from_utf8_lossy(&std::fs::read(path)?).into_owned();
        let title = rmd_core::title(&content).unwrap_or_else(|| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        let body = rmd_core::plain_text(&content);
        self.conn.execute(
            "UPDATE documents SET title = ?2, modified_at = ?3, missing = 0, indexed_at = ?4
             WHERE id = ?1",
            params![id.0, title, modified_at(path), now()],
        )?;
        self.conn
            .execute("DELETE FROM documents_fts WHERE rowid = ?1", [id.0])?;
        self.conn.execute(
            "INSERT INTO documents_fts (rowid, title, body) VALUES (?1, ?2, ?3)",
            params![id.0, title, body],
        )?;
        Ok(())
    }

    /// Drops documents that no longer belong to any environment.
    pub(crate) fn remove_orphan_documents(&self) -> Result<()> {
        self.conn.execute_batch(
            "DELETE FROM documents WHERE id NOT IN (SELECT document_id FROM environment_documents);
             DELETE FROM documents_fts WHERE rowid NOT IN (SELECT id FROM documents);",
        )?;
        self.remove_unused_tags()
    }
}

fn collect_markdown_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            if !name.starts_with('.') && !SKIPPED_DIRS.contains(&name.as_ref()) {
                collect_markdown_files(&path, out)?;
            }
        } else if file_type.is_file() && is_markdown(&path) {
            out.push(path);
        }
    }
    Ok(())
}

pub(crate) const SELECT_DOCUMENT: &str =
    "SELECT d.id, d.path, d.file_name, d.title, d.modified_at, d.missing FROM documents d";

pub(crate) fn row_to_document(row: &rusqlite::Row<'_>) -> rusqlite::Result<DocumentInfo> {
    Ok(DocumentInfo {
        id: DocumentId(row.get(0)?),
        path: PathBuf::from(row.get::<_, String>(1)?),
        file_name: row.get(2)?,
        title: row.get(3)?,
        modified_at: row.get(4)?,
        missing: row.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn fts_count(store: &Store, query: &str) -> i64 {
        store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM documents_fts WHERE documents_fts MATCH ?1",
                [query],
                |row| row.get(0),
            )
            .unwrap()
    }

    #[test]
    fn adds_and_indexes_documents() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("note.md");
        fs::write(&file, "# Réunion\n\nOrdre du jour : budget").unwrap();

        let store = Store::open_in_memory().unwrap();
        let env = store.create_environment("Work", "").unwrap().id;
        let id = store.add_document(env, &file).unwrap();

        let doc = store.document(id).unwrap();
        assert_eq!(doc.title, "Réunion");
        assert_eq!(doc.file_name, "note.md");
        assert!(doc.modified_at.is_some());
        assert_eq!(store.environment(env).unwrap().document_count, 1);
        // Accents are folded by the tokenizer.
        assert_eq!(fts_count(&store, "reunion"), 1);
        assert_eq!(fts_count(&store, "budget"), 1);

        // Adding twice does not duplicate.
        assert_eq!(store.add_document(env, &file).unwrap(), id);
        assert_eq!(store.environment(env).unwrap().document_count, 1);
    }

    #[test]
    fn shares_documents_between_environments() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("shared.md");
        fs::write(&file, "text").unwrap();

        let store = Store::open_in_memory().unwrap();
        let a = store.create_environment("A", "").unwrap().id;
        let b = store.create_environment("B", "").unwrap().id;
        let id = store.add_document(a, &file).unwrap();
        assert_eq!(store.add_document(b, &file).unwrap(), id);
        assert_eq!(store.environments_of(id).unwrap(), [a, b]);
        // Title falls back to the file stem.
        assert_eq!(store.document(id).unwrap().title, "shared");

        store.remove_document(a, id).unwrap();
        assert_eq!(store.environments_of(id).unwrap(), [b]);
        store.delete_environment(b).unwrap();
        assert!(matches!(store.document(id), Err(StoreError::NotFound)));
        assert_eq!(fts_count(&store, "text"), 0);
    }

    #[test]
    fn adds_folders_recursively() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("sub")).unwrap();
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join("a.md"), "a").unwrap();
        fs::write(dir.path().join("sub/b.MARKDOWN"), "b").unwrap();
        fs::write(dir.path().join("sub/c.txt"), "c").unwrap();
        fs::write(dir.path().join(".git/d.md"), "d").unwrap();

        let store = Store::open_in_memory().unwrap();
        let env = store.create_environment("Docs", "").unwrap().id;
        assert_eq!(store.add_folder(env, dir.path()).unwrap(), 2);
    }

    #[test]
    fn refresh_reindexes_changed_and_flags_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let kept = dir.path().join("kept.md");
        let gone = dir.path().join("gone.md");
        fs::write(&kept, "old words").unwrap();
        fs::write(&gone, "bye").unwrap();

        let store = Store::open_in_memory().unwrap();
        let env = store.create_environment("E", "").unwrap().id;
        let kept_id = store.add_document(env, &kept).unwrap();
        let gone_id = store.add_document(env, &gone).unwrap();

        // Force a different mtime than the indexed one.
        store
            .conn
            .execute(
                "UPDATE documents SET modified_at = 0 WHERE id = ?1",
                [kept_id.0],
            )
            .unwrap();
        fs::write(&kept, "fresh words").unwrap();
        fs::remove_file(&gone).unwrap();

        let report = store.refresh().unwrap();
        assert_eq!(
            report,
            RefreshReport {
                reindexed: 1,
                missing: 1
            }
        );
        assert_eq!(fts_count(&store, "fresh"), 1);
        assert_eq!(fts_count(&store, "old"), 0);
        assert!(store.document(gone_id).unwrap().missing);
    }

    #[test]
    fn rejects_missing_files() {
        let store = Store::open_in_memory().unwrap();
        let env = store.create_environment("E", "").unwrap().id;
        assert!(
            store
                .add_document(env, Path::new("does/not/exist.md"))
                .is_err()
        );
    }
}
