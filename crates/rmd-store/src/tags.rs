//! Tags attached to documents.
//!
//! Tags belong to the document, not to its environment membership: a file
//! tagged "draft" is a draft in every environment that references it.

use rusqlite::params;

use crate::{DocumentId, EnvironmentId, Result, Store};

/// A tag and the number of documents carrying it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagCount {
    pub name: String,
    pub count: usize,
}

/// Trims, drops empty entries and removes case-insensitive duplicates.
pub fn normalize_tags<S: AsRef<str>>(tags: &[S]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for tag in tags {
        let tag = tag.as_ref().trim().trim_start_matches('#').trim();
        if !tag.is_empty() && !out.iter().any(|known| known.eq_ignore_ascii_case(tag)) {
            out.push(tag.to_owned());
        }
    }
    out
}

impl Store {
    /// Replaces the tags of a document.
    pub fn set_tags<S: AsRef<str>>(&self, doc: DocumentId, tags: &[S]) -> Result<()> {
        self.document(doc)?;
        let tx = self.conn.unchecked_transaction()?;
        self.conn
            .execute("DELETE FROM document_tags WHERE document_id = ?1", [doc.0])?;
        for tag in normalize_tags(tags) {
            self.conn
                .execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", [&tag])?;
            self.conn.execute(
                "INSERT OR IGNORE INTO document_tags (document_id, tag_id)
                 SELECT ?1, id FROM tags WHERE name = ?2",
                params![doc.0, tag],
            )?;
        }
        self.remove_unused_tags()?;
        tx.commit()?;
        Ok(())
    }

    /// Tags of a document, sorted alphabetically.
    pub fn tags_of(&self, doc: DocumentId) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.name FROM tags t JOIN document_tags dt ON dt.tag_id = t.id
             WHERE dt.document_id = ?1 ORDER BY t.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([doc.0], |row| row.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Tags in use, optionally restricted to one environment, with counts.
    pub fn tags(&self, env: Option<EnvironmentId>) -> Result<Vec<TagCount>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.name, COUNT(DISTINCT dt.document_id) FROM tags t
             JOIN document_tags dt ON dt.tag_id = t.id
             WHERE ?1 IS NULL OR dt.document_id IN
                 (SELECT document_id FROM environment_documents WHERE environment_id = ?1)
             GROUP BY t.id ORDER BY t.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([env.map(|env| env.0)], |row| {
            Ok(TagCount {
                name: row.get(0)?,
                count: row.get::<_, i64>(1)? as usize,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub(crate) fn remove_unused_tags(&self) -> Result<()> {
        self.conn.execute(
            "DELETE FROM tags WHERE id NOT IN (SELECT tag_id FROM document_tags)",
            [],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn normalizes_tags() {
        assert_eq!(
            normalize_tags(&[" rust ", "#Rust", "", "  ", "notes"]),
            ["rust", "notes"]
        );
    }

    #[test]
    fn sets_lists_and_counts_tags() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.md"), "a").unwrap();
        fs::write(dir.path().join("b.md"), "b").unwrap();

        let store = Store::open_in_memory().unwrap();
        let work = store.create_environment("Work", "").unwrap().id;
        let home = store.create_environment("Home", "").unwrap().id;
        let a = store.add_document(work, &dir.path().join("a.md")).unwrap();
        let b = store.add_document(home, &dir.path().join("b.md")).unwrap();

        store.set_tags(a, &["urgent", "draft"]).unwrap();
        store.set_tags(b, &["Draft"]).unwrap();
        assert_eq!(store.tags_of(a).unwrap(), ["draft", "urgent"]);

        let all = store.tags(None).unwrap();
        assert_eq!(
            all,
            [
                TagCount {
                    name: "draft".into(),
                    count: 2
                },
                TagCount {
                    name: "urgent".into(),
                    count: 1
                },
            ]
        );
        let home_tags = store.tags(Some(home)).unwrap();
        assert_eq!(
            home_tags,
            [TagCount {
                name: "draft".into(),
                count: 1
            }]
        );

        // Unused tags disappear.
        store.set_tags(a, &["draft"]).unwrap();
        assert_eq!(store.tags(None).unwrap().len(), 1);
    }
}
