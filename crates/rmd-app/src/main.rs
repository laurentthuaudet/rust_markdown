// Hide the console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod document;
mod document_bar;
mod editor;
mod export;
mod find;
mod find_bar;
mod fonts;
mod icons;
mod instance;
mod library;
mod library_panel;
mod local_files;
mod notice;
mod settings;
mod tabs;
mod watcher;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder, WindowCloseBehaviour};

const APP_CSS: &str = include_str!("app.css");
/// Window icon, rendered from `assets/icon.svg` by build.rs.
const WINDOW_ICON: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/icon-256.png"));

fn main() {
    // A second launch hands its files to the running window and stops here.
    let files: Vec<std::path::PathBuf> = std::env::args_os().skip(1).map(Into::into).collect();
    let requests = match instance::claim("rust_markdown", &files) {
        instance::Instance::Primary(requests) => requests,
        instance::Instance::Forwarded => return,
    };

    let window = WindowBuilder::new()
        .with_title("Rust Markdown")
        .with_inner_size(LogicalSize::new(1280.0, 820.0))
        .with_window_icon(dioxus::desktop::icon_from_memory(WINDOW_ICON).ok());

    let config = Config::new()
        .with_window(window)
        .with_menu(None::<dioxus::desktop::muda::Menu>)
        .with_custom_head(format!(
            "<style>{}</style><style>{}</style><style>{APP_CSS}</style>",
            fonts::font_face_css(),
            rmd_core::STYLESHEET
        ))
        .with_custom_protocol(local_files::SCHEME, local_files::handle)
        // Closing only hides the window: the app decides whether to really
        // close once unsaved changes are dealt with (see `app::close_window`).
        .with_close_behaviour(WindowCloseBehaviour::WindowHides);

    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .with_context(requests)
        .launch(app::App);
}

#[cfg(test)]
mod tests {
    /// `assets/icon.ico` is committed for the installer (which needs a stable
    /// path) but must stay identical to the one build.rs renders from the SVG.
    /// If this fails, copy `$OUT_DIR/icon.ico` over `assets/icon.ico`.
    #[test]
    fn committed_installer_icon_matches_the_svg() {
        let generated = include_bytes!(concat!(env!("OUT_DIR"), "/icon.ico"));
        let committed = include_bytes!("../assets/icon.ico");
        assert!(generated == committed, "assets/icon.ico is out of date");
    }
}
