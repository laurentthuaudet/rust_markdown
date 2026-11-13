//! Standalone HTML export: a single self-contained file.
//!
//! Local images are embedded as `data:` URIs so the file can be moved or sent
//! without its surrounding folder.

use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rmd_core::RenderConfig;

use crate::{Document, LocalImage};

pub fn export(doc: &Document<'_>) -> String {
    let base_dir = doc.base_dir.clone();
    let config = RenderConfig {
        image_url_rewriter: Some(Arc::new(move |url: &str| {
            LocalImage::resolve(url, base_dir.as_deref())
                .and_then(|image| {
                    let bytes = image.read().ok()?;
                    Some(format!(
                        "data:{};base64,{}",
                        image.mime,
                        STANDARD.encode(bytes)
                    ))
                })
                .unwrap_or_else(|| url.to_owned())
        })),
    };
    let body = rmd_core::render_html_with(doc.markdown, &config);
    page(&doc.title, &body)
}

/// Wraps an HTML fragment in a complete page with the default stylesheet.
pub fn page(title: &str, body: &str) -> String {
    format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title}</title>\n<style>\n{css}</style>\n</head>\n\
         <body>\n<article class=\"markdown-body\">\n{body}</article>\n</body>\n</html>\n",
        title = escape(title),
        css = rmd_core::STYLESHEET,
    )
}

pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_a_standalone_page() {
        let doc = Document::new("# A <b> title\n\n| a |\n|---|\n| 1 |", None);
        let html = export(&doc);
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<title>A  title</title>"), "{html}");
        assert!(html.contains("<table>"));
        assert!(html.contains(".markdown-body"));
    }

    #[test]
    fn embeds_local_images() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pic.png"), b"\x89PNG").unwrap();
        let path = dir.path().join("doc.md");
        let doc = Document::new(
            "![pic](pic.png) ![remote](https://x.org/r.png)",
            Some(&path),
        );
        let html = export(&doc);
        assert!(
            html.contains("src=\"data:image/png;base64,iVBORw==\""),
            "{html}"
        );
        assert!(html.contains("src=\"https://x.org/r.png\""));
    }
}
