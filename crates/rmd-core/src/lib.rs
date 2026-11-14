//! Markdown parsing and HTML rendering (CommonMark + GFM, syntax highlighting).
//!
//! This crate is the single source of truth for how Markdown is interpreted:
//! the GUI preview, the exporters and the full-text indexer all go through it so
//! that a document looks and searches the same everywhere.

use std::panic::RefUnwindSafe;
use std::sync::{Arc, LazyLock};

mod footnotes;
mod highlight;

use comrak::nodes::{AstNode, NodeValue};
use comrak::options::{Plugins, URLRewriter};
use comrak::plugins::syntect::SyntectAdapterBuilder;
use comrak::{Arena, Options};

pub use comrak;
pub use highlight::{CodeTheme, highlight_css};

/// Base stylesheet for rendered documents, without code colours.
///
/// Colours are CSS variables on `.markdown-body` so a host (the app's dark
/// theme) can override them. See [`stylesheet`] for a complete light sheet.
pub const STYLESHEET: &str = include_str!("style.css");

/// Complete light stylesheet (base + code colours), used by the exporters.
pub fn stylesheet() -> String {
    format!("{STYLESHEET}\n{}", highlight_css(CodeTheme::Light))
}

/// Loading syntax definitions is relatively expensive, so do it once.
///
/// Code is highlighted with CSS classes (`hl-…`) rather than inline colours,
/// so the colour theme can follow the application theme.
static HIGHLIGHTER: LazyLock<highlight::ClassHighlighter> = LazyLock::new(|| {
    highlight::ClassHighlighter(
        SyntectAdapterBuilder::new()
            .css_with_class_prefix(highlight::CLASS_PREFIX)
            .build(),
    )
});

/// Rewrites the URL of images (e.g. to resolve paths relative to the document).
pub type UrlRewriter = Arc<dyn Fn(&str) -> String + Send + Sync + RefUnwindSafe>;

/// Tunables for [`render_html_with`].
#[derive(Clone, Default)]
pub struct RenderConfig {
    /// Optional rewriter applied to every image source.
    pub image_url_rewriter: Option<UrlRewriter>,
    /// Drop raw HTML instead of passing it through (needed for strict XHTML).
    pub strip_raw_html: bool,
    /// Add `data-sourcepos="line:col-line:col"` to block elements, so the
    /// preview can be scrolled in step with the editor.
    pub source_positions: bool,
}

struct FnRewriter(UrlRewriter);

impl URLRewriter for FnRewriter {
    fn to_html(&self, url: &str) -> String {
        (self.0)(url)
    }
}

/// Parsing/rendering options: CommonMark + GitHub Flavored Markdown.
///
/// Raw HTML is passed through, as in GitHub's renderer for local files: the
/// documents are the user's own and are only displayed locally.
pub fn options() -> Options<'static> {
    let mut options = Options::default();
    let ext = &mut options.extension;
    ext.strikethrough = true;
    ext.table = true;
    ext.autolink = true;
    ext.tasklist = true;
    ext.footnotes = true;
    ext.alerts = true;
    ext.header_id_prefix = Some(String::new());
    ext.front_matter_delimiter = Some("---".to_owned());
    // Keep footnote definitions where the author wrote them instead of
    // moving them all to the end of the document.
    options.parse.leave_footnote_definitions = true;
    options.render.r#unsafe = true;
    options.render.github_pre_lang = true;
    options.render.tasklist_classes = true;
    options
}

/// Renders Markdown to an HTML fragment (no `<html>`/`<body>` wrapper).
pub fn render_html(markdown: &str) -> String {
    render_html_with(markdown, &RenderConfig::default())
}

/// Renders Markdown to an HTML fragment with custom settings.
pub fn render_html_with(markdown: &str, config: &RenderConfig) -> String {
    let mut options = options();
    if let Some(rewriter) = &config.image_url_rewriter {
        options.extension.image_url_rewriter = Some(Arc::new(FnRewriter(rewriter.clone())));
    }
    options.render.sourcepos = config.source_positions;
    let mut plugins = Plugins::default();
    plugins.render.codefence_syntax_highlighter = Some(&*HIGHLIGHTER);

    let arena = Arena::new();
    let root = comrak::parse_document(&arena, markdown, &options);
    if config.strip_raw_html {
        // Removed from the tree rather than disabling raw HTML output, which
        // would also hide the footnote sections generated below.
        let raw: Vec<_> = root
            .descendants()
            .filter(|node| {
                matches!(
                    node.data().value,
                    NodeValue::HtmlBlock(_) | NodeValue::HtmlInline(_)
                )
            })
            .collect();
        for node in raw {
            node.detach();
        }
    }
    footnotes::place_in_place(&arena, root, &options, &plugins);

    let mut html = String::new();
    // Writing to a String cannot fail.
    let _ = comrak::format_html_with_plugins(root, &options, &mut html, &plugins);
    html
}

/// Returns the text of the first level-1 heading, or else of the first heading.
pub fn title(markdown: &str) -> Option<String> {
    let arena = Arena::new();
    let root = comrak::parse_document(&arena, markdown, &options());
    let mut first_any = None;
    for node in root.descendants() {
        if let NodeValue::Heading(heading) = node.data().value {
            let text = collect_text(node).trim().to_owned();
            if text.is_empty() {
                continue;
            }
            if heading.level == 1 {
                return Some(text);
            }
            first_any.get_or_insert(text);
        }
    }
    first_any
}

/// Extracts the readable text of a document, without markup.
///
/// Used to feed the full-text index. Blocks are separated by newlines so that
/// words from adjacent paragraphs do not merge.
pub fn plain_text(markdown: &str) -> String {
    let arena = Arena::new();
    let root = comrak::parse_document(&arena, markdown, &options());
    let mut out = String::with_capacity(markdown.len());
    for node in root.descendants() {
        let data = node.data();
        match &data.value {
            NodeValue::Text(text) => out.push_str(text),
            NodeValue::Code(code) => out.push_str(&code.literal),
            NodeValue::CodeBlock(block) => {
                out.push_str(&block.literal);
                out.push('\n');
            }
            NodeValue::SoftBreak | NodeValue::LineBreak => out.push(' '),
            value if value.block() && !out.ends_with('\n') && !out.is_empty() => out.push('\n'),
            _ => {}
        }
    }
    out.trim().to_owned()
}

/// Concatenates the text of all inline descendants of `node`.
pub fn collect_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut out = String::new();
    for child in node.descendants() {
        match &child.data().value {
            NodeValue::Text(text) => out.push_str(text),
            NodeValue::Code(code) => out.push_str(&code.literal),
            NodeValue::SoftBreak | NodeValue::LineBreak => out.push(' '),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_gfm_extensions() {
        let html = render_html(
            "| a | b |\n|---|---|\n| 1 | 2 |\n\n- [x] done\n- [ ] todo\n\n~~old~~ https://example.com",
        );
        assert!(html.contains("<table>"));
        assert!(html.contains("type=\"checkbox\""));
        assert!(html.contains("<del>old</del>"));
        assert!(html.contains("<a href=\"https://example.com\">"));
    }

    #[test]
    fn highlights_code_blocks() {
        let html = render_html("```rust\nfn main() {}\n```");
        assert!(
            html.contains("<pre class=\"syntax-highlighting\" lang=\"rust\">"),
            "{html}"
        );
        // Source positions survive on highlighted blocks (scroll sync).
        let config = RenderConfig {
            source_positions: true,
            ..Default::default()
        };
        let html = render_html_with("```rust\nfn main() {}\n```", &config);
        assert!(
            html.contains(
                "<pre class=\"syntax-highlighting\" data-sourcepos=\"1:1-3:3\" lang=\"rust\">"
            ),
            "{html}"
        );
        assert!(
            html.contains("<span class=\"hl-storage hl-type hl-function hl-rust\">fn</span>"),
            "{html}"
        );
    }

    #[test]
    fn headings_get_anchors() {
        let html = render_html("# Hello World");
        assert!(html.contains("id=\"hello-world\""), "{html}");
    }

    #[test]
    fn front_matter_is_not_rendered() {
        let html = render_html("---\ntags: [a]\n---\n\n# Title");
        assert!(!html.contains("tags"));
    }

    #[test]
    fn rewrites_image_urls() {
        let config = RenderConfig {
            image_url_rewriter: Some(Arc::new(|url: &str| format!("base/{url}"))),
            ..Default::default()
        };
        let html = render_html_with("![alt](img.png)", &config);
        assert!(html.contains("src=\"base/img.png\""), "{html}");
    }

    #[test]
    fn finds_title() {
        assert_eq!(
            title("## Sub\n\n# Main *title*").as_deref(),
            Some("Main title")
        );
        assert_eq!(title("## Only sub").as_deref(), Some("Only sub"));
        assert_eq!(title("no heading"), None);
    }

    #[test]
    fn extracts_plain_text() {
        let text = plain_text("# Title\n\nSome **bold** and `code`.\n\n- item one\n- item two");
        assert_eq!(text, "Title\nSome bold and code.\nitem one\nitem two");
    }
}

#[cfg(test)]
mod source_position_tests {
    use super::*;

    #[test]
    fn marks_blocks_with_source_lines_on_demand() {
        let markdown = "# Title\n\npara\n\n- item";
        assert!(!render_html(markdown).contains("data-sourcepos"));
        let config = RenderConfig {
            source_positions: true,
            ..Default::default()
        };
        let html = render_html_with(markdown, &config);
        assert!(html.contains("data-sourcepos=\"1:1-1:7\">Title"), "{html}");
        assert!(html.contains("<p data-sourcepos=\"3:1-3:4\""), "{html}");
    }
}
