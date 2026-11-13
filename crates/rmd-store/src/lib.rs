//! Persistent storage of environments (collections of Markdown files) backed by SQLite.
//!
//! An *environment* is a named collection that references Markdown files
//! anywhere on disk; a file can belong to several environments. The store
//! only keeps references and an index (title, modification date, full text):
//! the files themselves are never moved or modified.

mod documents;
mod environments;
mod schema;
mod tags;

use std::path::Path;

use rusqlite::Connection;

pub use documents::{DocumentId, DocumentInfo, RefreshReport, is_markdown};
pub use environments::{Environment, EnvironmentId};
pub use tags::{TagCount, normalize_tags};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("an environment named \"{0}\" already exists")]
    DuplicateEnvironment(String),
    #[error("the name cannot be empty")]
    EmptyName,
    #[error("not found")]
    NotFound,
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// Handle to the library database.
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Opens (or creates) the database at `path` and applies pending migrations.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        Self::init(Connection::open(path)?)
    }

    /// A throw-away database, for tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        schema::migrate(&conn)?;
        Ok(Self { conn })
    }
}

/// Current time as Unix seconds.
pub(crate) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Trims a user-supplied name and rejects empty ones.
pub(crate) fn clean_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        Err(StoreError::EmptyName)
    } else {
        Ok(name.to_owned())
    }
}
