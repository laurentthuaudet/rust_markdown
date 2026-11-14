//! DOCX (Word) export built with `docx-rs`.
//!
//! Markdown structure is mapped to real Word constructs — heading styles
//! (visible in the navigation pane), numbering definitions for lists, tables,
//! hyperlinks, footnotes and embedded pictures — rather than to direct
//! formatting only, so the result stays editable.

use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;

use docx_rs::{
    AbstractNumbering, AlignmentType, BorderType, BreakType, Docx, Footnote, Hyperlink,
    HyperlinkType, IndentLevel, Level, LevelJc, LevelText, LineSpacing, NumberFormat, Numbering,
    NumberingId, Paragraph, ParagraphBorder, ParagraphBorderPosition, Pic, Run, RunFonts, Shading,
    SpecialIndentType, Start, Style, StyleType, Table, TableCell, TableCellMargins, TableRow,
    VertAlignType,
};
use rmd_core::comrak::nodes::{AstNode, ListType, NodeValue, TableAlignment};
use rmd_core::comrak::{self, Arena};

use crate::{Document, ExportError, Format, LocalImage};

const BODY_FONT: &str = "Calibri";
const CODE_FONT: &str = "Consolas";
const CODE_BACKGROUND: &str = "F6F8FA";
const LINK_COLOR: &str = "0969DA";
const MUTED_COLOR: &str = "59636E";
/// Indentation step for lists and quotes, in twentieths of a point.
const INDENT: i32 = 720;
/// Maximum picture width: A4 width minus default margins (~16 cm), in EMU.
const MAX_PICTURE_WIDTH_EMU: u32 = 5_760_000;
const EMU_PER_PIXEL: u32 = 9_525;

pub fn export(doc: &Document<'_>) -> Result<Vec<u8>, ExportError> {
    let arena = Arena::new();
    let root = comrak::parse_document(&arena, doc.markdown, &rmd_core::options());

    let mut writer = Writer::new(doc.base_dir.as_deref());
    writer.collect_footnotes(root);
    writer.blocks(root, &Context::default());

    let mut docx = Docx::new()
        .default_fonts(
            RunFonts::new()
                .ascii(BODY_FONT)
                .hi_ansi(BODY_FONT)
                .cs(BODY_FONT),
        )
        .default_size(22)
        .default_line_spacing(LineSpacing::new().after(140).line(276));
    for style in styles() {
        docx = docx.add_style(style);
    }
    for (abstract_numbering, numbering) in writer.numberings {
        docx = docx
            .add_abstract_numbering(abstract_numbering)
            .add_numbering(numbering);
    }
    for item in writer.items {
        docx = match item {
            Item::Paragraph(paragraph) => docx.add_paragraph(*paragraph),
            Item::Table(table) => docx.add_table(*table),
        };
    }

    let mut buffer = Cursor::new(Vec::new());
    docx.build()
        .pack(&mut buffer)
        .map_err(|err| ExportError::backend(Format::Docx, err))?;
    Ok(buffer.into_inner())
}

/// Shaded run used for every piece of a code block.
fn code_run() -> Run {
    Run::new().shading(Shading::new().fill(CODE_BACKGROUND))
}
fn styles() -> Vec<Style> {
    let heading_sizes = [32, 26, 24, 22, 20, 20];
    let mut styles: Vec<Style> = heading_sizes
        .iter()
        .enumerate()
        .map(|(i, &size)| {
            Style::new(format!("Heading{}", i + 1), StyleType::Paragraph)
                .name(format!("heading {}", i + 1))
                .based_on("Normal")
                .next("Normal")
                .q_format(true)
                .outline_lvl(i)
                .size(size)
                .bold()
                .color("1F2328")
                .line_spacing(LineSpacing::new().before(280).after(100))
        })
        .collect();
    styles.push(
        Style::new("SourceCode", StyleType::Paragraph)
            .name("Source Code")
            .based_on("Normal")
            .fonts(
                RunFonts::new()
                    .ascii(CODE_FONT)
                    .hi_ansi(CODE_FONT)
                    .cs(CODE_FONT),
            )
            .size(19)
            .line_spacing(LineSpacing::new().after(140).line(240)),
    );
    styles.push(
        Style::new("Quote", StyleType::Paragraph)
            .name("Quote")
            .based_on("Normal")
            .color(MUTED_COLOR)
            .indent(Some(INDENT / 2), None, None, None),
    );
    styles.push(
        Style::new("Hyperlink", StyleType::Character)
            .name("Hyperlink")
            .color(LINK_COLOR)
            .underline("single"),
    );
    let mut footnote_reference =
        Style::new("FootnoteReference", StyleType::Character).name("footnote reference");
    footnote_reference.run_property = footnote_reference
        .run_property
        .vert_align(VertAlignType::SuperScript);
    styles.push(footnote_reference);
    styles
}

enum Item {
    Paragraph(Box<Paragraph>),
    Table(Box<Table>),
}

/// Block-level context: list nesting and quote depth.
#[derive(Clone, Default)]
struct Context {
    /// Numbering instance and level of the enclosing list item, if any.
    list: Option<(usize, usize)>,
    /// Whether the next paragraph is the first one of a list item (gets the bullet).
    first_in_item: bool,
    /// Nesting depth of lists (for continuation paragraph indentation).
    list_depth: usize,
    quote_depth: usize,
}

/// Inline formatting state.
#[derive(Clone, Default)]
struct InlineStyle {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    link: bool,
}

// Pieces are short-lived (one paragraph at a time), boxing would only add noise.
#[allow(clippy::large_enum_variant)]
enum Piece {
    Run(Run),
    Link(Hyperlink),
}

struct Writer<'p> {
    base_dir: Option<&'p Path>,
    items: Vec<Item>,
    numberings: Vec<(AbstractNumbering, Numbering)>,
    footnotes: HashMap<String, Vec<Paragraph>>,
}

impl<'p> Writer<'p> {
    fn new(base_dir: Option<&'p Path>) -> Self {
        Self {
            base_dir,
            items: Vec::new(),
            numberings: Vec::new(),
            footnotes: HashMap::new(),
        }
    }

    fn collect_footnotes<'a>(&mut self, root: &'a AstNode<'a>) {
        for node in root.descendants() {
            let name = match &node.data().value {
                NodeValue::FootnoteDefinition(def) => def.name.clone(),
                _ => continue,
            };
            let paragraphs = node
                .children()
                .filter(|child| matches!(child.data().value, NodeValue::Paragraph))
                .map(|child| self.add_inlines(Paragraph::new(), child, &InlineStyle::default()))
                .collect();
            self.footnotes.insert(name, paragraphs);
        }
    }

    fn push_paragraph(&mut self, paragraph: Paragraph) {
        self.items.push(Item::Paragraph(Box::new(paragraph)));
    }

    /// Applies list/quote indentation to a block paragraph.
    fn decorate(&self, mut paragraph: Paragraph, ctx: &mut Context) -> Paragraph {
        if ctx.list.is_some() {
            paragraph = paragraph.line_spacing(LineSpacing::new().after(40));
        }
        if ctx.quote_depth > 0 {
            paragraph = paragraph.style("Quote");
        }
        let quote_indent = ctx.quote_depth as i32 * INDENT / 2;
        match ctx.list {
            Some((num_id, level)) if ctx.first_in_item => {
                ctx.first_in_item = false;
                paragraph = paragraph.numbering(NumberingId::new(num_id), IndentLevel::new(level));
            }
            _ => {
                let left = ctx.list_depth as i32 * INDENT + quote_indent;
                if left > 0 {
                    paragraph = paragraph.indent(Some(left), None, None, None);
                }
            }
        }
        paragraph
    }

    fn blocks<'a>(&mut self, node: &'a AstNode<'a>, ctx: &Context) {
        let mut ctx = ctx.clone();
        for child in node.children() {
            self.block(child, &mut ctx);
        }
    }

    fn block<'a>(&mut self, node: &'a AstNode<'a>, ctx: &mut Context) {
        let value = node.data().value.clone();
        match value {
            NodeValue::Paragraph => {
                let paragraph = self.decorate(Paragraph::new(), ctx);
                let paragraph = self.add_inlines(paragraph, node, &InlineStyle::default());
                self.push_paragraph(paragraph);
            }
            NodeValue::Heading(heading) => {
                let style = format!("Heading{}", heading.level.clamp(1, 6));
                let paragraph = Paragraph::new().style(&style);
                let paragraph = self.add_inlines(paragraph, node, &InlineStyle::default());
                self.push_paragraph(paragraph);
            }
            NodeValue::CodeBlock(code) => {
                let lang = code.info.split_whitespace().next().unwrap_or("");
                let literal = code.literal.strip_suffix('\n').unwrap_or(&code.literal);
                let mut paragraph = self.decorate(Paragraph::new().style("SourceCode"), ctx);
                // One coloured run per token, same palette as the HTML export.
                for (i, line) in rmd_core::highlight_code(literal, lang)
                    .into_iter()
                    .enumerate()
                {
                    if i > 0 {
                        paragraph =
                            paragraph.add_run(code_run().add_break(BreakType::TextWrapping));
                    }
                    for token in line {
                        let (r, g, b) = token.color;
                        let mut run = code_run()
                            .color(format!("{r:02X}{g:02X}{b:02X}"))
                            .add_text(token.text);
                        if token.bold {
                            run = run.bold();
                        }
                        if token.italic {
                            run = run.italic();
                        }
                        paragraph = paragraph.add_run(run);
                    }
                }
                self.push_paragraph(paragraph);
            }
            NodeValue::BlockQuote | NodeValue::MultilineBlockQuote(_) => {
                let mut inner = ctx.clone();
                inner.quote_depth += 1;
                self.blocks(node, &inner);
            }
            NodeValue::Alert(alert) => {
                let mut inner = ctx.clone();
                inner.quote_depth += 1;
                let title = alert
                    .title
                    .clone()
                    .unwrap_or_else(|| alert.alert_type.default_title().to_owned());
                let paragraph = self.decorate(Paragraph::new(), &mut inner);
                self.push_paragraph(paragraph.add_run(Run::new().bold().add_text(title)));
                self.blocks(node, &inner);
            }
            NodeValue::List(list) => {
                let level = ctx.list.map_or(0, |(_, level)| level + 1).min(8);
                let num_id = self.add_numbering(list.list_type, list.start);
                for item in node.children() {
                    let mut inner = ctx.clone();
                    inner.list = Some((num_id, level));
                    inner.first_in_item = true;
                    inner.list_depth = level + 1;
                    if let NodeValue::TaskItem(task) = item.data().value {
                        let mark = if task.symbol.is_some() {
                            "☒ "
                        } else {
                            "☐ "
                        };
                        self.task_item(item, &mut inner, mark);
                    } else {
                        self.blocks(item, &inner);
                    }
                }
            }
            NodeValue::Table(table) => self.table(node, &table.alignments),
            NodeValue::ThematicBreak => {
                let mut paragraph = Paragraph::new();
                paragraph.property = paragraph.property.set_border(
                    ParagraphBorder::new(ParagraphBorderPosition::Bottom)
                        .val(BorderType::Single)
                        .size(6)
                        .color("D1D9E0"),
                );
                self.push_paragraph(paragraph);
            }
            NodeValue::HtmlBlock(_)
            | NodeValue::FrontMatter(_)
            | NodeValue::FootnoteDefinition(_) => {}
            _ => self.blocks(node, ctx),
        }
    }

    /// Task list items: the checkbox is prepended to the first paragraph.
    fn task_item<'a>(&mut self, item: &'a AstNode<'a>, ctx: &mut Context, mark: &str) {
        for (i, child) in item.children().enumerate() {
            if i == 0 && matches!(child.data().value, NodeValue::Paragraph) {
                let paragraph = self
                    .decorate(Paragraph::new(), ctx)
                    .add_run(Run::new().add_text(mark));
                let paragraph = self.add_inlines(paragraph, child, &InlineStyle::default());
                self.push_paragraph(paragraph);
            } else {
                self.block(child, ctx);
            }
        }
    }

    /// Creates a numbering definition for one list and returns its id.
    fn add_numbering(&mut self, list_type: ListType, start: usize) -> usize {
        let id = self.numberings.len() + 1;
        let mut abstract_numbering = AbstractNumbering::new(id);
        for level in 0..9 {
            let (format, text) = match list_type {
                ListType::Bullet => ("bullet", ["•", "◦", "▪"][level % 3].to_owned()),
                ListType::Ordered => ("decimal", format!("%{}.", level + 1)),
            };
            let level_start = if level == 0 { start.max(1) } else { 1 };
            abstract_numbering = abstract_numbering.add_level(
                Level::new(
                    level,
                    Start::new(level_start),
                    NumberFormat::new(format),
                    LevelText::new(text),
                    LevelJc::new("left"),
                )
                .indent(
                    Some(INDENT * (level as i32 + 1)),
                    Some(SpecialIndentType::Hanging(360)),
                    None,
                    None,
                ),
            );
        }
        self.numberings
            .push((abstract_numbering, Numbering::new(id, id)));
        id
    }

    fn table<'a>(&mut self, node: &'a AstNode<'a>, alignments: &[TableAlignment]) {
        let mut rows = Vec::new();
        for row in node.children() {
            let header = matches!(row.data().value, NodeValue::TableRow(true));
            let mut cells = Vec::new();
            for (i, cell) in row.children().enumerate() {
                let align = match alignments.get(i) {
                    Some(TableAlignment::Center) => AlignmentType::Center,
                    Some(TableAlignment::Right) => AlignmentType::Right,
                    _ => AlignmentType::Left,
                };
                let format = InlineStyle {
                    bold: header,
                    ..InlineStyle::default()
                };
                let paragraph = self.add_inlines(
                    Paragraph::new()
                        .align(align)
                        .line_spacing(LineSpacing::new().after(0)),
                    cell,
                    &format,
                );
                let mut table_cell = TableCell::new().add_paragraph(paragraph);
                if header {
                    table_cell = table_cell.shading(Shading::new().fill(CODE_BACKGROUND));
                }
                cells.push(table_cell);
            }
            rows.push(TableRow::new(cells));
        }
        self.items.push(Item::Table(Box::new(
            Table::new(rows).margins(TableCellMargins::new().margin(40, 110, 40, 110)),
        )));
        // Word glues consecutive tables together: keep a spacer paragraph.
        self.push_paragraph(Paragraph::new());
    }

    fn add_inlines<'a>(
        &mut self,
        mut paragraph: Paragraph,
        node: &'a AstNode<'a>,
        format: &InlineStyle,
    ) -> Paragraph {
        let mut pieces = Vec::new();
        self.inlines(node, format, &mut pieces);
        for piece in pieces {
            paragraph = match piece {
                Piece::Run(run) => paragraph.add_run(run),
                Piece::Link(link) => paragraph.add_hyperlink(link),
            };
        }
        paragraph
    }

    fn inlines<'a>(&mut self, node: &'a AstNode<'a>, format: &InlineStyle, out: &mut Vec<Piece>) {
        for child in node.children() {
            self.inline(child, format, out);
        }
    }

    fn text_run(text: &str, format: &InlineStyle) -> Run {
        let mut run = Run::new().add_text(text);
        if format.bold {
            run = run.bold();
        }
        if format.italic {
            run = run.italic();
        }
        if format.strike {
            run = run.strike();
        }
        if format.code {
            run = run
                .fonts(
                    RunFonts::new()
                        .ascii(CODE_FONT)
                        .hi_ansi(CODE_FONT)
                        .cs(CODE_FONT),
                )
                .shading(Shading::new().fill(CODE_BACKGROUND));
        }
        if format.link {
            run = run.style("Hyperlink");
        }
        run
    }

    fn inline<'a>(&mut self, node: &'a AstNode<'a>, format: &InlineStyle, out: &mut Vec<Piece>) {
        let value = node.data().value.clone();
        let nested = |change: fn(&mut InlineStyle)| {
            let mut inner = format.clone();
            change(&mut inner);
            inner
        };
        match value {
            NodeValue::Text(text) => out.push(Piece::Run(Self::text_run(&text, format))),
            NodeValue::Code(code) => {
                let inner = nested(|f| f.code = true);
                out.push(Piece::Run(Self::text_run(&code.literal, &inner)));
            }
            NodeValue::SoftBreak => out.push(Piece::Run(Self::text_run(" ", format))),
            NodeValue::LineBreak => {
                out.push(Piece::Run(Run::new().add_break(BreakType::TextWrapping)))
            }
            NodeValue::Emph => self.inlines(node, &nested(|f| f.italic = true), out),
            NodeValue::Strong => self.inlines(node, &nested(|f| f.bold = true), out),
            NodeValue::Strikethrough => self.inlines(node, &nested(|f| f.strike = true), out),
            NodeValue::Link(link) => {
                if link.url.is_empty() || link.url.starts_with('#') {
                    self.inlines(node, format, out);
                    return;
                }
                let mut inner = Vec::new();
                self.inlines(node, &nested(|f| f.link = true), &mut inner);
                let mut hyperlink = Hyperlink::new(&link.url, HyperlinkType::External);
                for piece in inner {
                    if let Piece::Run(run) = piece {
                        hyperlink = hyperlink.add_run(run);
                    }
                }
                out.push(Piece::Link(hyperlink));
            }
            NodeValue::Image(link) => {
                let alt = rmd_core::collect_text(node);
                match self.picture(&link.url) {
                    Some(pic) => out.push(Piece::Run(Run::new().add_image(pic))),
                    None => {
                        let label = if alt.is_empty() {
                            link.url.clone()
                        } else {
                            alt
                        };
                        let run = Self::text_run(&label, &nested(|f| f.link = true));
                        out.push(Piece::Link(
                            Hyperlink::new(&link.url, HyperlinkType::External).add_run(run),
                        ));
                    }
                }
            }
            NodeValue::FootnoteReference(reference) => {
                let mut footnote = Footnote::new();
                for paragraph in self
                    .footnotes
                    .get(&reference.name)
                    .cloned()
                    .unwrap_or_default()
                {
                    footnote = footnote.add_content(paragraph);
                }
                out.push(Piece::Run(Run::new().add_footnote_reference(footnote)));
            }
            NodeValue::HtmlInline(html) => {
                if html.trim().to_ascii_lowercase().starts_with("<br") {
                    out.push(Piece::Run(Run::new().add_break(BreakType::TextWrapping)));
                }
            }
            _ => self.inlines(node, format, out),
        }
    }

    /// Loads a local raster image as a picture scaled to fit the page width.
    fn picture(&self, url: &str) -> Option<Pic> {
        let image = LocalImage::resolve(url, self.base_dir)?;
        let bytes = image.read().ok()?;
        let (pic, width, height) = if image.extension == "svg" {
            // Word pictures need a raster image: render the SVG to PNG.
            let svg = crate::svg::rasterize(&bytes, image.path.parent())?;
            let pic = Pic::new_with_dimensions(svg.png, svg.pixel_width, svg.pixel_height);
            (pic, svg.width, svg.height)
        } else {
            // `Pic::new` panics on undecodable data: validate first.
            let decoded = image::load_from_memory(&bytes).ok()?;
            let (width, height) = (decoded.width().max(1), decoded.height().max(1));
            (Pic::new(&bytes), width, height)
        };
        let width_emu = width * EMU_PER_PIXEL;
        let height_emu = height * EMU_PER_PIXEL;
        Some(if width_emu > MAX_PICTURE_WIDTH_EMU {
            let scaled_height = (u64::from(height_emu) * u64::from(MAX_PICTURE_WIDTH_EMU)
                / u64::from(width_emu)) as u32;
            pic.size(MAX_PICTURE_WIDTH_EMU, scaled_height)
        } else {
            pic.size(width_emu, height_emu)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    fn document_xml(markdown: &str) -> String {
        let doc = Document::new(markdown, None);
        let bytes = export(&doc).unwrap();
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut xml = String::new();
        zip.by_name("word/document.xml")
            .unwrap()
            .read_to_string(&mut xml)
            .unwrap();
        xml
    }

    #[test]
    fn exports_demo_document() {
        let markdown = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/demo.md"
        ))
        .unwrap();
        let xml = document_xml(&markdown);
        assert!(xml.contains("Heading1"));
        assert!(xml.contains("<w:tbl>"));
        assert!(xml.contains("w:hyperlink"));
        assert!(xml.contains("Bonjour, Markdown"));
    }

    #[test]
    fn maps_inline_formatting() {
        let xml = document_xml("**bold** *it* ~~del~~ `code`");
        assert!(xml.contains("<w:b />") || xml.contains("<w:b/>"), "{xml}");
        assert!(xml.contains("<w:i />") || xml.contains("<w:i/>"));
        assert!(xml.contains("<w:strike />") || xml.contains("<w:strike/>"));
        assert!(xml.contains(CODE_FONT));
    }

    #[test]
    fn embeds_svg_images_as_bitmaps() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("diagram.svg"),
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="300" height="100"><rect width="300" height="100" fill="#09d"/></svg>"##,
        )
        .unwrap();
        let path = dir.path().join("doc.md");
        let doc = Document::new("![diagram](diagram.svg)", Some(&path));
        let bytes = export(&doc).unwrap();

        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let media: Vec<String> = zip
            .file_names()
            .filter(|name| name.starts_with("word/media/") && name.ends_with(".png"))
            .map(str::to_owned)
            .collect();
        assert_eq!(media.len(), 1, "{media:?}");
        let mut xml = String::new();
        zip.by_name("word/document.xml")
            .unwrap()
            .read_to_string(&mut xml)
            .unwrap();
        assert!(xml.contains("<w:drawing>"), "{xml}");
        // Displayed at the SVG's own size (300 px), not the 2x bitmap.
        assert!(
            xml.contains(&format!("cx=\"{}\"", 300 * EMU_PER_PIXEL)),
            "{xml}"
        );
    }

    #[test]
    fn colours_code_blocks() {
        let xml = document_xml("```rust\nfn main() {}\nlet x = 1;\n```");
        // Several colours (one per token kind) and the line break is kept.
        let colours: std::collections::HashSet<&str> = xml
            .match_indices("<w:color w:val=\"")
            .map(|(i, tag)| &xml[i + tag.len()..i + tag.len() + 6])
            .collect();
        assert!(colours.len() >= 2, "{colours:?}");
        assert!(xml.contains(">fn</w:t>"), "{xml}");
        assert!(xml.contains("textWrapping"), "{xml}");
    }

    #[test]
    fn lists_use_numbering() {
        let xml = document_xml("1. one\n2. two\n\n- a\n  - nested");
        assert_eq!(xml.matches("<w:numPr>").count(), 4, "{xml}");
        assert!(xml.contains("<w:ilvl w:val=\"1\" />") || xml.contains("<w:ilvl w:val=\"1\"/>"));
    }
}
