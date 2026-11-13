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
