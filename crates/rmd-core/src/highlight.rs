//! CSS for syntax-highlighted code blocks, in a light and a dark variant.

use std::sync::LazyLock;

use syntect::highlighting::{Color, ThemeSet};
use syntect::html::{ClassStyle, css_for_theme_with_class_style};

/// Prefix of the classes emitted on highlighted code.
pub(crate) const CLASS_PREFIX: &str = "hl-";

/// Colour theme for code blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodeTheme {
    Light,
    Dark,
}

impl CodeTheme {
    fn syntect_name(self) -> &'static str {
        match self {
            CodeTheme::Light => "InspiredGitHub",
            CodeTheme::Dark => "base16-ocean.dark",
        }
    }
}

static CSS: LazyLock<[String; 2]> = LazyLock::new(|| {
    let themes = ThemeSet::load_defaults();
    [CodeTheme::Light, CodeTheme::Dark].map(|code_theme| {
        let theme = &themes.themes[code_theme.syntect_name()];
        let style = ClassStyle::SpacedPrefixed {
            prefix: CLASS_PREFIX,
        };
        let tokens = css_for_theme_with_class_style(theme, style).unwrap_or_default();
        let hex = |color: Color| format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b);
        let background = theme.settings.background.map(hex);
        let foreground = theme.settings.foreground.map(hex);
        // comrak marks highlighted blocks with `<pre class="syntax-highlighting">`.
        format!(
            "{tokens}\n.markdown-body pre.syntax-highlighting {{ background: {}; color: {}; }}\n",
            background.as_deref().unwrap_or("transparent"),
            foreground.as_deref().unwrap_or("inherit"),
        )
    })
});

/// Stylesheet colouring highlighted code blocks with the given theme.
pub fn highlight_css(theme: CodeTheme) -> &'static str {
    match theme {
        CodeTheme::Light => &CSS[0],
        CodeTheme::Dark => &CSS[1],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_both_themes() {
        let light = highlight_css(CodeTheme::Light);
        let dark = highlight_css(CodeTheme::Dark);
        assert!(light.contains(".hl-keyword"));
        assert!(dark.contains(".hl-keyword"));
        assert!(
            dark.contains("pre.syntax-highlighting { background: #2b303b;"),
            "{dark}"
        );
        assert_ne!(light, dark);
    }
}
