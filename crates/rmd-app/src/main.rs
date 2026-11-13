// Hide the console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod document;
mod local_files;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};

const APP_CSS: &str = include_str!("app.css");

fn main() {
    let window = WindowBuilder::new()
        .with_title("Rust Markdown")
        .with_inner_size(LogicalSize::new(1280.0, 820.0));

    let config = Config::new()
        .with_window(window)
        .with_menu(None::<dioxus::desktop::muda::Menu>)
        .with_custom_head(format!(
            "<style>{}</style><style>{APP_CSS}</style>",
            rmd_core::STYLESHEET
        ))
        .with_custom_protocol(local_files::SCHEME, local_files::handle);

    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .launch(app::App);
}
