//! CSS for syntax-highlighted code blocks, in a light and a dark variant.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::sync::LazyLock;

use comrak::adapters::SyntaxHighlighterAdapter;
use comrak::plugins::syntect::SyntectAdapter;

use syntect::easy::HighlightLines;
use syntect::highlighting::{Color, FontStyle, Style, Theme, ThemeSet};
use syntect::html::{ClassStyle, css_for_theme_with_class_style};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

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

/// A run of code text sharing one style, for exporters that cannot use CSS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeToken {
    pub text: String,
    /// RGB colour.
    pub color: (u8, u8, u8),
    pub bold: bool,
    pub italic: bool,
}

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static LIGHT_THEME: LazyLock<Theme> = LazyLock::new(|| {
    ThemeSet::load_defaults()
        .themes
        .remove(CodeTheme::Light.syntect_name())
        .unwrap_or_default()
});

/// Highlights `code` with the light theme, one token list per line (without
/// the line breaks). Unknown languages come back as plain, uncoloured text.
pub fn highlight_code(code: &str, lang: &str) -> Vec<Vec<CodeToken>> {
    let syntax = SYNTAXES
        .find_syntax_by_token(lang)
        .unwrap_or_else(|| SYNTAXES.find_syntax_plain_text());
    let mut highlighter = HighlightLines::new(syntax, &LIGHT_THEME);
    LinesWithEndings::from(code)
        .map(|line| {
            let regions = highlighter
                .highlight_line(line, &SYNTAXES)
                .unwrap_or_else(|_| vec![(Style::default(), line)]);
            regions
                .into_iter()
                .filter_map(|(style, text)| {
                    let text = text.trim_end_matches(['\n', '\r']);
                    (!text.is_empty()).then(|| CodeToken {
                        text: text.to_owned(),
                        color: (style.foreground.r, style.foreground.g, style.foreground.b),
                        bold: style.font_style.contains(FontStyle::BOLD),
                        italic: style.font_style.contains(FontStyle::ITALIC),
                    })
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_code_into_styled_tokens() {
        let lines = highlight_code("fn main() {}\n// done\n", "rust");
        assert_eq!(lines.len(), 2);
        let keyword = &lines[0][0];
        assert_eq!(keyword.text, "fn");
        let plain = lines[0]
            .iter()
            .find(|token| token.text.contains("main"))
            .unwrap();
        assert_ne!(keyword.color, plain.color);
        let joined: String = lines[1].iter().map(|token| token.text.as_str()).collect();
        assert_eq!(joined, "// done");

        // Unknown language: still split into lines, text preserved.
        let lines = highlight_code("a\nb", "nope");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1][0].text, "b");
    }

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
