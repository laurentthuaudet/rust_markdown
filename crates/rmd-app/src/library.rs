//! Shared access to the environment library (SQLite store).

use std::path::PathBuf;
use std::rc::Rc;

use chrono::{Local, NaiveDate, TimeZone};
use dioxus::prelude::*;
use rmd_store::{Store, StoreError};

/// Library handle shared through the Dioxus context.
///
/// `version` is bumped after every change so that views reading it recompute.
#[derive(Clone)]
pub struct Library {
    store: Result<Rc<Store>, Rc<String>>,
    pub version: Signal<u64>,
}

impl Library {
    /// Opens the library database in `%APPDATA%\rust_markdown`.
    pub fn open() -> Self {
        let store = database_path()
            .ok_or_else(|| "APPDATA is not set".to_owned())
            .and_then(|path| Store::open(path).map_err(|err| err.to_string()))
            .map(|store| {
                // Pick up files edited outside the application.
                let _ = store.refresh();
                Rc::new(store)
            })
            .map_err(Rc::new);
        Self {
            store,
            version: Signal::new(0),
        }
    }

    /// Error that prevented the library from opening, if any.
    pub fn error(&self) -> Option<String> {
        self.store.as_ref().err().map(|err| err.to_string())
    }

    /// Runs a read-only query, subscribing the caller to library changes.
    pub fn read<T>(&self, query: impl FnOnce(&Store) -> rmd_store::Result<T>) -> Option<T> {
        self.version.read();
        let store = self.store.as_ref().ok()?;
        query(store).ok()
    }

    /// Runs a mutation and notifies readers.
    pub fn write<T>(
        &self,
        change: impl FnOnce(&Store) -> rmd_store::Result<T>,
    ) -> Result<T, String> {
        let store = self.store.as_ref().map_err(|err| err.to_string())?;
        let result = change(store).map_err(|err| describe(&err));
        let mut version = self.version;
        version += 1;
        result
    }
}

fn database_path() -> Option<PathBuf> {
    let app_data = std::env::var_os("APPDATA")?;
    Some(
        PathBuf::from(app_data)
            .join("rust_markdown")
            .join("library.db"),
    )
}

/// User-facing (French) description of a store error.
pub fn describe(err: &StoreError) -> String {
    match err {
        StoreError::DuplicateEnvironment(name) => {
            format!("Un environnement nommé « {name} » existe déjà.")
        }
        StoreError::EmptyName => "Le nom ne peut pas être vide.".to_owned(),
        StoreError::NotFound => "Élément introuvable.".to_owned(),
        StoreError::Io(err) => format!("Erreur de fichier : {err}"),
        StoreError::Sqlite(err) => format!("Erreur de base de données : {err}"),
    }
}

/// Start of the given local day (`YYYY-MM-DD`), as Unix seconds.
pub fn day_start(date: &str) -> Option<i64> {
    local_midnight(NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?)
}

/// Last second of the given local day (`YYYY-MM-DD`), as Unix seconds.
///
/// Computed from the next midnight, so days with a DST change are handled.
pub fn day_end(date: &str) -> Option<i64> {
    let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    Some(local_midnight(date.succ_opt()?)? - 1)
}

fn local_midnight(date: NaiveDate) -> Option<i64> {
    let local = Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0)?)
        .earliest()?;
    Some(local.timestamp())
}

/// Formats a Unix time as a short local date and time.
pub fn format_time(secs: i64) -> String {
    Local
        .timestamp_opt(secs, 0)
        .single()
        .map(|time| time.format("%d/%m/%Y %H:%M").to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_local_days() {
        let start = day_start("2026-03-14").unwrap();
        let end = day_end("2026-03-14").unwrap();
        assert_eq!(end + 1, day_start("2026-03-15").unwrap());
        assert!(end > start);
        assert_eq!(&format_time(start)[..10], "14/03/2026");
        assert_eq!(day_start("not a date"), None);
    }
}
