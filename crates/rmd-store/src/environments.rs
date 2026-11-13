//! Environments: named collections of documents.

use rusqlite::{ErrorCode, OptionalExtension, params};

use crate::{Result, Store, StoreError, clean_name, now};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EnvironmentId(pub i64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Environment {
    pub id: EnvironmentId,
    pub name: String,
    pub description: String,
    pub created_at: i64,
    /// Number of documents referenced by the environment.
    pub document_count: usize,
}

/// Maps a uniqueness violation on the name to a friendly error.
fn unique_name(err: rusqlite::Error, name: &str) -> StoreError {
    match err.sqlite_error_code() {
        Some(ErrorCode::ConstraintViolation) => StoreError::DuplicateEnvironment(name.to_owned()),
        _ => err.into(),
    }
}

impl Store {
    pub fn create_environment(&self, name: &str, description: &str) -> Result<Environment> {
        let name = clean_name(name)?;
        self.conn
            .execute(
                "INSERT INTO environments (name, description, created_at) VALUES (?1, ?2, ?3)",
                params![name, description.trim(), now()],
            )
            .map_err(|err| unique_name(err, &name))?;
        self.environment(EnvironmentId(self.conn.last_insert_rowid()))
    }

    pub fn update_environment(
        &self,
        id: EnvironmentId,
        name: &str,
        description: &str,
    ) -> Result<Environment> {
        let name = clean_name(name)?;
        let changed = self
            .conn
            .execute(
                "UPDATE environments SET name = ?2, description = ?3 WHERE id = ?1",
                params![id.0, name, description.trim()],
            )
            .map_err(|err| unique_name(err, &name))?;
        if changed == 0 {
            return Err(StoreError::NotFound);
        }
        self.environment(id)
    }

    /// Deletes the environment. The referenced files are left untouched;
    /// documents no longer in any environment are dropped from the index.
    pub fn delete_environment(&self, id: EnvironmentId) -> Result<()> {
        let changed = self
            .conn
            .execute("DELETE FROM environments WHERE id = ?1", [id.0])?;
        if changed == 0 {
            return Err(StoreError::NotFound);
        }
        self.remove_orphan_documents()
    }

    pub fn environment(&self, id: EnvironmentId) -> Result<Environment> {
        self.conn
            .query_row(
                &format!("{SELECT_ENVIRONMENT} WHERE e.id = ?1"),
                [id.0],
                row_to_environment,
            )
            .optional()?
            .ok_or(StoreError::NotFound)
    }

    /// All environments, sorted by name.
    pub fn environments(&self) -> Result<Vec<Environment>> {
        let mut stmt = self.conn.prepare(&format!(
            "{SELECT_ENVIRONMENT} ORDER BY e.name COLLATE NOCASE"
        ))?;
        let rows = stmt.query_map([], row_to_environment)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

const SELECT_ENVIRONMENT: &str = "SELECT e.id, e.name, e.description, e.created_at,
            (SELECT COUNT(*) FROM environment_documents ed WHERE ed.environment_id = e.id)
     FROM environments e";

fn row_to_environment(row: &rusqlite::Row<'_>) -> rusqlite::Result<Environment> {
    Ok(Environment {
        id: EnvironmentId(row.get(0)?),
        name: row.get(1)?,
        description: row.get(2)?,
        created_at: row.get(3)?,
        document_count: row.get::<_, i64>(4)? as usize,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_lists_updates_and_deletes() {
        let store = Store::open_in_memory().unwrap();
        let work = store.create_environment("  Work ", "notes").unwrap();
        assert_eq!(work.name, "Work");
        store.create_environment("archive", "").unwrap();

        let names: Vec<_> = store
            .environments()
            .unwrap()
            .into_iter()
            .map(|env| env.name)
            .collect();
        assert_eq!(names, ["archive", "Work"]);

        let renamed = store.update_environment(work.id, "Job", "").unwrap();
        assert_eq!(renamed.name, "Job");

        store.delete_environment(work.id).unwrap();
        assert!(matches!(
            store.environment(work.id),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn rejects_duplicate_and_empty_names() {
        let store = Store::open_in_memory().unwrap();
        store.create_environment("Work", "").unwrap();
        assert!(matches!(
            store.create_environment("WORK", ""),
            Err(StoreError::DuplicateEnvironment(_))
        ));
        assert!(matches!(
            store.create_environment("   ", ""),
            Err(StoreError::EmptyName)
        ));
    }
}
