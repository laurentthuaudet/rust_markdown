//! User preferences, persisted in `%APPDATA%\rust_markdown\settings.json`.

use std::path::PathBuf;

use dioxus::prelude::*;
use rmd_core::CodeTheme;
use serde::{Deserialize, Serialize};

/// Colour theme chosen by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Follow the Windows light/dark setting.
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    /// Next theme when cycling with the toolbar button.
    pub fn next(self) -> Self {
        match self {
            Theme::System => Theme::Light,
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::System,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Theme::System => "◐ Système",
            Theme::Light => "☀ Clair",
            Theme::Dark => "☾ Sombre",
        }
    }

    /// Whether the dark palette applies, given the system preference.
    pub fn is_dark(self, system_dark: bool) -> bool {
        match self {
            Theme::System => system_dark,
            Theme::Light => false,
            Theme::Dark => true,
        }
    }

    pub fn code_theme(self, system_dark: bool) -> CodeTheme {
        if self.is_dark(system_dark) {
            CodeTheme::Dark
        } else {
            CodeTheme::Light
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Theme,
    /// Show line numbers in the editor gutter.
    pub line_numbers: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            line_numbers: true,
        }
    }
}

fn settings_path() -> Option<PathBuf> {
    let app_data = std::env::var_os("APPDATA")?;
    Some(
        PathBuf::from(app_data)
            .join("rust_markdown")
            .join("settings.json"),
    )
}

impl Settings {
    /// Loads the saved settings; defaults when missing or unreadable.
    pub fn load() -> Self {
        settings_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    /// Best-effort save: preferences are not worth an error message.
    pub fn save(&self) {
        let Some(path) = settings_path() else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}

/// Tracks the Windows light/dark preference (live, through the web view).
pub fn use_system_dark() -> Signal<bool> {
    let mut dark = use_signal(|| false);
    use_future(move || async move {
        let mut eval = document::eval(
            r#"
            const query = matchMedia("(prefers-color-scheme: dark)");
            dioxus.send(query.matches);
            query.addEventListener("change", (event) => dioxus.send(event.matches));
            await new Promise(() => {}); // keep the channel open
            "#,
        );
        while let Ok(value) = eval.recv::<bool>().await {
            dark.set(value);
        }
    });
    dark
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_and_cycles_themes() {
        assert!(Theme::System.is_dark(true));
        assert!(!Theme::System.is_dark(false));
        assert!(Theme::Dark.is_dark(false));
        assert_eq!(Theme::System.next().next().next(), Theme::System);
    }

    #[test]
    fn tolerates_partial_or_unknown_json() {
        let settings: Settings = serde_json::from_str(r#"{"theme":"dark","other":1}"#).unwrap();
        assert_eq!(settings.theme, Theme::Dark);
        assert!(settings.line_numbers);
    }
}
