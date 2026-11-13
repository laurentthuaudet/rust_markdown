//! EPUB 3 export built with `epub-builder`.
//!
//! The document becomes one XHTML chapter whose table of contents mirrors the
//! heading hierarchy. Local images are packaged inside the book.

use std::sync::{Arc, Mutex};

use epub_builder::{EpubBuilder, EpubContent, EpubVersion, TocElement, ZipLibrary};
use rmd_core::RenderConfig;
use rmd_core::comrak::nodes::NodeValue;
use rmd_core::comrak::{self, Anchorizer, Arena};

use crate::html::escape;
use crate::{Document, ExportError, Format, LocalImage};

const CHAPTER: &str = "content.xhtml";

pub fn export(doc: &Document<'_>) -> Result<Vec<u8>, ExportError> {
    let fail = |err: epub_builder::Error| ExportError::backend(Format::Epub, err);

    let images: Arc<Mutex<Vec<LocalImage>>> = Arc::default();
    let base_dir = doc.base_dir.clone();
    let collected = images.clone();
    let config = RenderConfig {
        image_url_rewriter: Some(Arc::new(move |url: &str| {
            match LocalImage::resolve(url, base_dir.as_deref()) {
                Some(image) => {
                    let mut images = collected.lock().expect("image list lock");
                    let path = format!("images/{}.{}", images.len(), image.extension);
                    images.push(image);
                    path
                }
                None => url.to_owned(),
            }
        })),
        strip_raw_html: true,
        ..Default::default()
    };
    let body = xhtml_fixups(&rmd_core::render_html_with(doc.markdown, &config));

    let mut builder = EpubBuilder::new(ZipLibrary::new().map_err(fail)?).map_err(fail)?;
    builder.epub_version(EpubVersion::V30);
    builder.set_title(doc.title.clone());
    builder.set_generator("rust_markdown");
    builder
        .stylesheet(rmd_core::stylesheet().as_bytes())
        .map_err(fail)?;

    let images = std::mem::take(&mut *images.lock().expect("image list lock"));
    for (i, image) in images.iter().enumerate() {
        let bytes = image.read()?;
        let path = format!("images/{i}.{}", image.extension);
        builder
            .add_resource(path, bytes.as_slice(), image.mime)
            .map_err(fail)?;
    }

    let page = chapter_page(&doc.title, &body);
    let mut content = EpubContent::new(CHAPTER, page.as_bytes());
    content.toc = table_of_contents(doc);
    builder.add_content(content).map_err(fail)?;

    let mut out = Vec::new();
    builder.generate(&mut out).map_err(fail)?;
    Ok(out)
}

fn chapter_page(title: &str, body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE html>\n\
         <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">\n\
         <head>\n<meta charset=\"UTF-8\" />\n<title>{}</title>\n\
         <link rel=\"stylesheet\" type=\"text/css\" href=\"stylesheet.css\" />\n</head>\n\
         <body>\n<article class=\"markdown-body\">\n{body}</article>\n</body>\n</html>\n",
        escape(title)
    )
}

/// comrak emits a few boolean attributes without value, which XHTML forbids.
fn xhtml_fixups(html: &str) -> String {
    html.replace(" data-footnotes>", " data-footnotes=\"\">")
        .replace(" data-footnote-ref>", " data-footnote-ref=\"\">")
        .replace(" data-footnote-backref ", " data-footnote-backref=\"\" ")
}

/// Builds the TOC from the headings, linking to the ids comrak generates.
fn table_of_contents(doc: &Document<'_>) -> TocElement {
    let arena = Arena::new();
    let root = comrak::parse_document(&arena, doc.markdown, &rmd_core::options());
    let mut anchorizer = Anchorizer::new();
    let mut toc = TocElement::new(CHAPTER, doc.title.clone()).level(0);
    for node in root.descendants() {
        let NodeValue::Heading(heading) = node.data().value else {
            continue;
        };
        let text = rmd_core::collect_text(node);
        let id = anchorizer.anchorize(&text);
        if text.trim().is_empty() {
            continue;
        }
        toc.add(
            TocElement::new(format!("{CHAPTER}#{id}"), text.trim()).level(i32::from(heading.level)),
        );
    }
    toc
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read};

    use super::*;

    fn read_entry(epub: &[u8], name: &str) -> String {
        let mut zip = zip::ZipArchive::new(Cursor::new(epub)).unwrap();
        let mut out = String::new();
        zip.by_name(name).unwrap().read_to_string(&mut out).unwrap();
        out
    }

    #[test]
    fn builds_a_valid_book() {
        let markdown = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/demo.md"
        ))
        .unwrap();
        let doc = Document::new(&markdown, None);
        let epub = export(&doc).unwrap();

        assert_eq!(read_entry(&epub, "mimetype"), "application/epub+zip");
        let chapter = read_entry(&epub, "OEBPS/content.xhtml");
        assert!(chapter.contains("<table>"));
        assert!(!chapter.contains("data-footnotes>"));
        let nav = read_entry(&epub, "OEBPS/nav.xhtml");
        assert!(nav.contains("content.xhtml#liste-de-tâches"), "{nav}");
    }

    #[test]
    fn packages_local_images() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pic.png"), b"png").unwrap();
        let path = dir.path().join("doc.md");
        let doc = Document::new("# T\n\n![pic](pic.png)", Some(&path));
        let epub = export(&doc).unwrap();
        let chapter = read_entry(&epub, "OEBPS/content.xhtml");
        assert!(chapter.contains("src=\"images/0.png\""), "{chapter}");
        let opf = read_entry(&epub, "OEBPS/content.opf");
        assert!(opf.contains("images/0.png"));
    }

    #[test]
    fn strips_raw_html() {
        let doc = Document::new("<div>raw</div>\n\ntext", None);
        let epub = export(&doc).unwrap();
        assert!(!read_entry(&epub, "OEBPS/content.xhtml").contains("<div>raw"));
    }
}
