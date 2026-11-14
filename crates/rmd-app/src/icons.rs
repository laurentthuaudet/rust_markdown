//! Interface icons, following Caretta: line icons on a 24 px grid, 1.5 px
//! stroke, round caps and joins, drawn in the current text colour.

use dioxus::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Close,
    Plus,
    Refresh,
    ChevronDown,
    ChevronRight,
    ArrowUp,
    ArrowDown,
    Sun,
    Moon,
    Monitor,
    Sidebar,
    Info,
    Alert,
}

impl Icon {
    fn path(self) -> &'static str {
        match self {
            Icon::Close => "M18 6 6 18M6 6l12 12",
            Icon::Plus => "M12 5v14M5 12h14",
            Icon::Refresh => "M20 12a8 8 0 1 1-2.34-5.66L20 8.5M20 4v4.5h-4.5",
            Icon::ChevronDown => "m6 9 6 6 6-6",
            Icon::ChevronRight => "m9 6 6 6-6 6",
            Icon::ArrowUp => "M12 19V5M5 12l7-7 7 7",
            Icon::ArrowDown => "M12 5v14M19 12l-7 7-7-7",
            Icon::Sun => {
                "M12 8a4 4 0 1 0 0 8a4 4 0 1 0 0-8M12 2v2M12 20v2M4.9 4.9l1.4 1.4\
                 M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"
            }
            Icon::Moon => "M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z",
            Icon::Monitor => {
                "M5 4h14a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zM8 20h8M12 16v4"
            }
            Icon::Sidebar => {
                "M5 4h14a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zM9 4v16"
            }
            Icon::Info => "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18M12 11v5M12 8h.01",
            Icon::Alert => {
                "M10.3 4.2 2.6 17.5A2 2 0 0 0 4.3 20.5h15.4a2 2 0 0 0 1.7-3L13.7 4.2a2 2 0 0 0-3.4 0zM12 9v4M12 17h.01"
            }
        }
    }
}

/// An inline SVG icon; decorative (hidden from screen readers) — buttons
/// carry their own `title` / text.
#[component]
pub fn Glyph(icon: Icon, #[props(default = 16)] size: u32) -> Element {
    rsx! {
        svg {
            class: "icon-svg",
            width: "{size}",
            height: "{size}",
            view_box: "0 0 24 24",
            "aria-hidden": "true",
            path {
                d: icon.path(),
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.5",
                stroke_linecap: "round",
                stroke_linejoin: "round",
            }
        }
    }
}
