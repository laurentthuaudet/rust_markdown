//! Finding documents by environment, name, tags, modification date and content.

use rusqlite::ToSql;

use crate::documents::{DocumentInfo, row_to_document};
use crate::{EnvironmentId, Result, Store};

/// Marks the start of a matched term in [`SearchHit::snippet`].
pub const MATCH_START: char = '\u{2}';
/// Marks the end of a matched term in [`SearchHit::snippet`].
pub const MATCH_END: char = '\u{3}';

/// Search criteria. Every criterion left empty is ignored; the others are combined with AND.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// Restrict to one environment.
    pub environment: Option<EnvironmentId>,
    /// Case-insensitive substring of the file name or title.
    pub name: String,
    /// Documents must carry all of these tags.
    pub tags: Vec<String>,
    /// Modified at or after this Unix time (seconds).
    pub modified_from: Option<i64>,
    /// Modified at or before this Unix time (seconds).
    pub modified_to: Option<i64>,
    /// Full-text query: every word must appear (prefix match, accent-insensitive).
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub document: DocumentInfo,
    pub tags: Vec<String>,
    /// Excerpt around the full-text matches, with matches wrapped in
    /// [`MATCH_START`]/[`MATCH_END`]. Only set for full-text searches.
    pub snippet: Option<String>,
}

/// Turns free text into a safe FTS5 query: each word becomes a quoted prefix term.
fn fts_query(text: &str) -> Option<String> {
    let terms: Vec<String> = text
        .split_whitespace()
        .map(|word| word.replace('"', ""))
        .filter(|word| !word.is_empty())
        .map(|word| format!("\"{word}\"*"))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

fn like_pattern(text: &str) -> String {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

impl Store {
    /// Documents matching `filter`: best full-text matches first, otherwise
    /// most recently modified first.
    pub fn search(&self, filter: &Filter) -> Result<Vec<SearchHit>> {
        let mut sql =
            String::from("SELECT d.id, d.path, d.file_name, d.title, d.modified_at, d.missing");
        let mut params: Vec<Box<dyn ToSql>> = Vec::new();
        let text = fts_query(&filter.text);

        if text.is_some() {
            sql.push_str(&format!(
                ", snippet(documents_fts, 1, '{MATCH_START}', '{MATCH_END}', '…', 16)
                 FROM documents d JOIN documents_fts ON documents_fts.rowid = d.id
                 WHERE documents_fts MATCH ?"
            ));
        } else {
            sql.push_str(", NULL FROM documents d WHERE 1 = 1");
        }
        if let Some(query) = text.clone() {
            params.push(Box::new(query));
        }
        if let Some(env) = filter.environment {
            sql.push_str(
                " AND d.id IN (SELECT document_id FROM environment_documents
                               WHERE environment_id = ?)",
            );
            params.push(Box::new(env.0));
        }
        let name = filter.name.trim();
        if !name.is_empty() {
            sql.push_str(" AND (d.file_name LIKE ? ESCAPE '\\' OR d.title LIKE ? ESCAPE '\\')");
            params.push(Box::new(like_pattern(name)));
            params.push(Box::new(like_pattern(name)));
        }
        if let Some(from) = filter.modified_from {
            sql.push_str(" AND d.modified_at >= ?");
            params.push(Box::new(from));
        }
        if let Some(to) = filter.modified_to {
            sql.push_str(" AND d.modified_at <= ?");
            params.push(Box::new(to));
        }
        for tag in crate::normalize_tags(&filter.tags) {
            sql.push_str(
                " AND d.id IN (SELECT dt.document_id FROM document_tags dt
                               JOIN tags t ON t.id = dt.tag_id WHERE t.name = ?)",
            );
            params.push(Box::new(tag));
        }
        sql.push_str(if text.is_some() {
            " ORDER BY bm25(documents_fts, 5.0, 1.0), d.modified_at DESC"
        } else {
            " ORDER BY d.modified_at DESC, d.title COLLATE NOCASE"
        });

        let mut stmt = self.conn.prepare(&sql)?;
        let params: Vec<&dyn ToSql> = params.iter().map(|param| param.as_ref()).collect();
        let rows: Vec<(DocumentInfo, Option<String>)> = stmt
            .query_map(params.as_slice(), |row| {
                Ok((row_to_document(row)?, row.get(6)?))
            })?
            .collect::<rusqlite::Result<_>>()?;

        rows.into_iter()
            .map(|(document, snippet)| {
                Ok(SearchHit {
                    tags: self.tags_of(document.id)?,
                    document,
                    snippet,
                })
            })
            .collect()
    }

    /// All documents of an environment, most recently modified first.
    pub fn documents_in(&self, env: EnvironmentId) -> Result<Vec<SearchHit>> {
        self.search(&Filter {
            environment: Some(env),
            ..Filter::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;
    use crate::DocumentId;

    struct Fixture {
        store: Store,
        work: EnvironmentId,
        home: EnvironmentId,
        _dir: tempfile::TempDir,
    }

    fn add(store: &Store, env: EnvironmentId, dir: &Path, name: &str, body: &str) -> DocumentId {
        let path = dir.join(name);
        fs::write(&path, body).unwrap();
        store.add_document(env, &path).unwrap()
    }

    fn set_mtime(store: &Store, doc: DocumentId, mtime: i64) {
        store
            .conn
            .execute(
                "UPDATE documents SET modified_at = ?2 WHERE id = ?1",
                rusqlite::params![doc.0, mtime],
            )
            .unwrap();
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let work = store.create_environment("Work", "").unwrap().id;
        let home = store.create_environment("Home", "").unwrap().id;

        let budget = add(
            &store,
            work,
            dir.path(),
            "budget.md",
            "# Budget 2026\n\nPrévisions de dépenses.",
        );
        let meeting = add(
            &store,
            work,
            dir.path(),
            "meeting.md",
            "# Réunion\n\nOn parle du budget.",
        );
        let recipes = add(
            &store,
            home,
            dir.path(),
            "recipes.md",
            "# Recettes\n\nGâteau 100% chocolat.",
        );
        store
            .add_document(home, &dir.path().join("meeting.md"))
            .unwrap();

        store.set_tags(budget, &["finance", "2026"]).unwrap();
        store.set_tags(meeting, &["finance"]).unwrap();
        store.set_tags(recipes, &["cuisine"]).unwrap();
        set_mtime(&store, budget, 1_000);
        set_mtime(&store, meeting, 2_000);
        set_mtime(&store, recipes, 3_000);

        Fixture {
            store,
            work,
            home,
            _dir: dir,
        }
    }

    fn titles(hits: &[SearchHit]) -> Vec<&str> {
        hits.iter().map(|hit| hit.document.title.as_str()).collect()
    }

    #[test]
    fn lists_an_environment_by_recency() {
        let f = fixture();
        let hits = f.store.documents_in(f.work).unwrap();
        assert_eq!(titles(&hits), ["Réunion", "Budget 2026"]);
        assert_eq!(hits[1].tags, ["2026", "finance"]);
        assert_eq!(
            titles(&f.store.documents_in(f.home).unwrap()),
            ["Recettes", "Réunion"]
        );
    }

    #[test]
    fn filters_by_name_tags_and_dates() {
        let f = fixture();
        let by_name = Filter {
            name: "BUD".into(),
            ..Filter::default()
        };
        assert_eq!(titles(&f.store.search(&by_name).unwrap()), ["Budget 2026"]);

        let literal_percent = Filter {
            name: "%".into(),
            ..Filter::default()
        };
        assert!(f.store.search(&literal_percent).unwrap().is_empty());

        let by_tags = Filter {
            tags: vec!["finance".into(), "2026".into()],
            ..Filter::default()
        };
        assert_eq!(titles(&f.store.search(&by_tags).unwrap()), ["Budget 2026"]);

        let by_date = Filter {
            modified_from: Some(1_500),
            modified_to: Some(2_500),
            ..Filter::default()
        };
        assert_eq!(titles(&f.store.search(&by_date).unwrap()), ["Réunion"]);

        let combined = Filter {
            environment: Some(f.home),
            tags: vec!["finance".into()],
            ..Filter::default()
        };
        assert_eq!(titles(&f.store.search(&combined).unwrap()), ["Réunion"]);
    }

    #[test]
    fn searches_full_text_with_snippets() {
        let f = fixture();
        let filter = Filter {
            text: "budg".into(),
            ..Filter::default()
        };
        let hits = f.store.search(&filter).unwrap();
        // Title matches rank first.
        assert_eq!(titles(&hits), ["Budget 2026", "Réunion"]);
        let snippet = hits[1].snippet.as_deref().unwrap();
        assert!(
            snippet.contains(&format!("{MATCH_START}budget{MATCH_END}")),
            "{snippet}"
        );

        // Accent-insensitive, every word required, quotes are harmless.
        let filter = Filter {
            text: "gateau \"chocolat".into(),
            ..Filter::default()
        };
        assert_eq!(titles(&f.store.search(&filter).unwrap()), ["Recettes"]);
        let filter = Filter {
            text: "gateau budget".into(),
            ..Filter::default()
        };
        assert!(f.store.search(&filter).unwrap().is_empty());
    }

    #[test]
    fn builds_safe_fts_queries() {
        assert_eq!(fts_query("  "), None);
        assert_eq!(fts_query("a \"b OR"), Some("\"a\"* \"b\"* \"OR\"*".into()));
    }
}
