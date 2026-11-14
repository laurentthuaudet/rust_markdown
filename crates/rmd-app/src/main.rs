// Hide the console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod document;
mod document_bar;
mod editor;
mod export;
mod find;
mod find_bar;
mod library;
mod library_panel;
mod local_files;
mod notice;
mod settings;
mod watcher;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder, WindowCloseBehaviour};

const APP_CSS: &str = include_str!("app.css");
/// Window icon, rendered from `assets/icon.svg` by build.rs.
const WINDOW_ICON: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/icon-256.png"));

fn main() {
    let window = WindowBuilder::new()
        .with_title("Rust Markdown")
        .with_inner_size(LogicalSize::new(1280.0, 820.0))
        .with_window_icon(dioxus::desktop::icon_from_memory(WINDOW_ICON).ok());

    let config = Config::new()
        .with_window(window)
        .with_menu(None::<dioxus::desktop::muda::Menu>)
        .with_custom_head(format!(
            "<style>{}</style><style>{APP_CSS}</style>",
            rmd_core::STYLESHEET
        ))
        .with_custom_protocol(local_files::SCHEME, local_files::handle)
        // Closing only hides the window: the app decides whether to really
        // close once unsaved changes are dealt with (see `app::close_window`).
        .with_close_behaviour(WindowCloseBehaviour::WindowHides);

    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .launch(app::App);
}
