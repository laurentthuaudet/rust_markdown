//! CSS for syntax-highlighted code blocks, in a light and a dark variant.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::sync::LazyLock;

use comrak::adapters::SyntaxHighlighterAdapter;
use comrak::plugins::syntect::SyntectAdapter;

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

/// Wraps comrak's syntect adapter, which in CSS-class mode writes a bare
/// `<pre class="syntax-highlighting">` and drops the attributes comrak passes
/// (`lang`, `data-sourcepos`). This keeps them and adds the class.
pub(crate) struct ClassHighlighter(pub(crate) SyntectAdapter);

impl SyntaxHighlighterAdapter for ClassHighlighter {
    fn write_highlighted(
        &self,
        output: &mut dyn fmt::Write,
        lang: Option<&str>,
        code: &str,
    ) -> fmt::Result {
        self.0.write_highlighted(output, lang, code)
    }

    fn write_pre_tag(
        &self,
        output: &mut dyn fmt::Write,
        mut attributes: HashMap<&'static str, Cow<'_, str>>,
    ) -> fmt::Result {
        let class = match attributes.get("class") {
            Some(existing) => format!("syntax-highlighting {existing}"),
            None => "syntax-highlighting".to_owned(),
        };
        attributes.insert("class", class.into());
        // Stable attribute order (HashMap iteration order is random).
        let mut attributes: Vec<_> = attributes.into_iter().collect();
        attributes.sort_by_key(|(name, _)| *name);
        comrak::html::write_opening_tag(output, "pre", attributes)
    }

    fn write_code_tag(
        &self,
        output: &mut dyn fmt::Write,
        attributes: HashMap<&'static str, Cow<'_, str>>,
    ) -> fmt::Result {
        self.0.write_code_tag(output, attributes)
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
