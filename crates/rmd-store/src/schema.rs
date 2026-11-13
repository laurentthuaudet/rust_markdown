//! Database schema and migrations, tracked with `PRAGMA user_version`.

use rusqlite::Connection;

/// Each entry upgrades the schema from version `index` to `index + 1`.
const MIGRATIONS: &[&str] = &[
    // 1: environments.
    "CREATE TABLE environments (
        id          INTEGER PRIMARY KEY,
        name        TEXT NOT NULL UNIQUE COLLATE NOCASE,
        description TEXT NOT NULL DEFAULT '',
        created_at  INTEGER NOT NULL
    );",
    // 2: documents, environment membership and full-text index.
    "CREATE TABLE documents (
        id          INTEGER PRIMARY KEY,
        path        TEXT NOT NULL UNIQUE COLLATE NOCASE,
        file_name   TEXT NOT NULL,
        title       TEXT NOT NULL DEFAULT '',
        modified_at INTEGER,
        missing     INTEGER NOT NULL DEFAULT 0,
        indexed_at  INTEGER NOT NULL
    );
    CREATE TABLE environment_documents (
        environment_id INTEGER NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
        document_id    INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
        added_at       INTEGER NOT NULL,
        PRIMARY KEY (environment_id, document_id)
    );
    CREATE INDEX environment_documents_by_document ON environment_documents(document_id);
    CREATE VIRTUAL TABLE documents_fts USING fts5(
        title, body, tokenize = 'unicode61 remove_diacritics 2'
    );",
    // 3: tags attached to documents.
    "CREATE TABLE tags (
        id   INTEGER PRIMARY KEY,
        name TEXT NOT NULL UNIQUE COLLATE NOCASE
    );
    CREATE TABLE document_tags (
        document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
        tag_id      INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
        PRIMARY KEY (document_id, tag_id)
    );
    CREATE INDEX document_tags_by_tag ON document_tags(tag_id);",
];

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    for (index, migration) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(migration)?;
        tx.pragma_update(None, "user_version", index as i64 + 1)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
    }
}
