//! PDF export through the Typst typesetting engine, used as a library.

use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

use typst::diag::{FileError, FileResult, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;

use crate::typst_markup;
use crate::{Document, ExportError, Format};

/// Windows fonts used by the default styles, loaded when present.
const SYSTEM_FONTS: &[&str] = &[
    "segoeui.ttf",
    "segoeuib.ttf",
    "segoeuii.ttf",
    "segoeuiz.ttf",
    "seguisb.ttf",
    "seguisym.ttf",
    "seguiemj.ttf",
    "consola.ttf",
    "consolab.ttf",
    "consolai.ttf",
    "consolaz.ttf",
    "CascadiaCode.ttf",
];

struct Fonts {
    fonts: Vec<Font>,
    book: LazyHash<FontBook>,
}

/// Fonts are expensive to load and shared by every export.
static FONTS: LazyLock<Fonts> = LazyLock::new(|| {
    let mut fonts: Vec<Font> = typst_assets::fonts()
        .flat_map(|data| Font::iter(Bytes::new(data)))
        .collect();
    let windows = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
    let font_dir = Path::new(&windows).join("Fonts");
    for name in SYSTEM_FONTS {
        if let Ok(data) = std::fs::read(font_dir.join(name)) {
            fonts.extend(Font::iter(Bytes::new(data)));
        }
    }
    let book = LazyHash::new(FontBook::from_fonts(&fonts));
    Fonts { fonts, book }
});

static LIBRARY: LazyLock<LazyHash<Library>> = LazyLock::new(|| LazyHash::new(Library::default()));

/// A minimal Typst world: one in-memory source and a set of image files.
struct MarkdownWorld {
    main: Source,
    files: HashMap<String, Bytes>,
}

impl MarkdownWorld {
    fn new(source: String, files: HashMap<String, Bytes>) -> Self {
        let path = VirtualPath::new("/main.typ").expect("valid virtual path");
        let id = RootedPath::new(VirtualRoot::Project, path).intern();
        Self {
            main: Source::new(id, source),
            files,
        }
    }
}

impl World for MarkdownWorld {
    fn library(&self) -> &LazyHash<Library> {
        &LIBRARY
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &FONTS.book
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else {
            Err(not_found(id))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.files
            .get(id.vpath().get_with_slash())
            .cloned()
            .ok_or_else(|| not_found(id))
    }

    fn font(&self, index: usize) -> Option<Font> {
        FONTS.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

fn not_found(id: FileId) -> FileError {
    FileError::NotFound(id.vpath().get_with_slash().into())
}

fn describe(errors: &[SourceDiagnostic]) -> String {
    errors
        .iter()
        .map(|diag| diag.message.to_string())
        .collect::<Vec<_>>()
        .join("; ")
}

pub fn export(doc: &Document<'_>) -> Result<Vec<u8>, ExportError> {
    let converted = typst_markup::convert(doc.markdown, &doc.title, doc.base_dir.as_deref());
    let mut files = HashMap::new();
    for (vpath, image) in converted.images {
        files.insert(vpath, Bytes::new(image.read()?));
    }
    let world = MarkdownWorld::new(converted.source, files);

    let document: PagedDocument = typst::compile(&world)
        .output
        .map_err(|errors| ExportError::backend(Format::Pdf, describe(&errors)))?;
    typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default())
        .map_err(|errors| ExportError::backend(Format::Pdf, describe(&errors)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_a_pdf() {
        let markdown = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/demo.md"
        ))
        .unwrap();
        let doc = Document::new(&markdown, None);
        let pdf = export(&doc).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
    }

    #[test]
    fn embeds_local_images() {
        let dir = tempfile::tempdir().unwrap();
        const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="red"/></svg>"#;
        std::fs::write(dir.path().join("dot.svg"), SVG).unwrap();
        let path = dir.path().join("doc.md");
        let doc = Document::new("# Img\n\n![dot](dot.svg)", Some(&path));
        let pdf = export(&doc).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
    }
}
