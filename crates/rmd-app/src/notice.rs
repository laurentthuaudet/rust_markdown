//! Messages shown in the status bar.

use dioxus::prelude::*;

use crate::icons::{Glyph, Icon};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Info,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub text: String,
}

impl Notice {
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            kind: NoticeKind::Info,
            text: text.into(),
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            kind: NoticeKind::Error,
            text: text.into(),
        }
    }
}

/// Shared status bar state.
pub type Status = Signal<Option<Notice>>;

#[component]
pub fn StatusBar(status: Status) -> Element {
    let Some(notice) = status() else {
        return rsx! {};
    };
    let class = match notice.kind {
        NoticeKind::Info => "status info",
        NoticeKind::Error => "status error",
    };
    rsx! {
        footer { class,
            Glyph { icon: if notice.kind == NoticeKind::Error { Icon::Alert } else { Icon::Info } }
            span { class: "status-text", "{notice.text}" }
            button { class: "icon", title: "Fermer", onclick: move |_| status.set(None), Glyph { icon: Icon::Close } }
        }
    }
}
