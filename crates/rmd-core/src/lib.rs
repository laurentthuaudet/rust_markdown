//! Markdown parsing and HTML rendering (CommonMark + GFM, syntax highlighting).
//!
//! This crate is the single source of truth for how Markdown is interpreted:
//! the GUI preview, the exporters and the full-text indexer all go through it so
//! that a document looks and searches the same everywhere.

use std::panic::RefUnwindSafe;
use std::sync::{Arc, LazyLock};

use comrak::nodes::{AstNode, NodeValue};
use comrak::options::{Plugins, URLRewriter};
use comrak::plugins::syntect::SyntectAdapter;
use comrak::{Arena, Options};

pub use comrak;

/// Syntect theme used for inline-styled code blocks.
const HIGHLIGHT_THEME: &str = "InspiredGitHub";

/// Default stylesheet for rendered documents (preview, HTML and EPUB exports).
pub const STYLESHEET: &str = include_str!("style.css");

/// Loading syntax definitions is relatively expensive, so do it once.
static HIGHLIGHTER: LazyLock<SyntectAdapter> =
    LazyLock::new(|| SyntectAdapter::new(Some(HIGHLIGHT_THEME)));

/// Rewrites the URL of images (e.g. to resolve paths relative to the document).
pub type UrlRewriter = Arc<dyn Fn(&str) -> String + Send + Sync + RefUnwindSafe>;

/// Tunables for [`render_html_with`].
#[derive(Clone, Default)]
pub struct RenderConfig {
    /// Optional rewriter applied to every image source.
    pub image_url_rewriter: Option<UrlRewriter>,
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
    let mut plugins = Plugins::default();
    plugins.render.codefence_syntax_highlighter = Some(&*HIGHLIGHTER);
    comrak::markdown_to_html_with_plugins(markdown, &options, &plugins)
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
        assert!(html.contains("lang=\"rust\""));
        assert!(html.contains("style=\"color:"), "{html}");
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
