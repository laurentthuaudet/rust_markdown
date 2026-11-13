//! Conversion of a Markdown AST into Typst markup (used by the PDF exporter).
//!
//! All document text is emitted as Typst string literals (`#"..."`), so no
//! Markdown content can ever be interpreted as Typst syntax: only the two
//! characters `\` and `"` need escaping.

use std::collections::HashMap;
use std::fmt::Write;
use std::path::Path;

use rmd_core::comrak::nodes::{AstNode, ListType, NodeValue, TableAlignment};
use rmd_core::comrak::{self, Arena};

use crate::LocalImage;

/// Page setup and styles shared by every exported document.
const PREAMBLE: &str = r##"#set page(paper: "a4", margin: (x: 2.2cm, y: 2.5cm), numbering: "1")
#set text(font: ("Segoe UI", "Libertinus Serif", "DejaVu Sans"), size: 10.5pt, fill: rgb("#1f2328"))
#set par(justify: true, leading: 0.7em, spacing: 1.1em)
#set heading(numbering: none)
#show heading: set block(above: 1.6em, below: 0.8em)
#show heading.where(level: 1): it => block(width: 100%, stroke: (bottom: 0.6pt + rgb("#d1d9e0")), inset: (bottom: 0.3em), text(size: 1.6em, it))
#show heading.where(level: 2): it => block(width: 100%, stroke: (bottom: 0.6pt + rgb("#d1d9e0")), inset: (bottom: 0.3em), text(size: 1.3em, it))
#show raw: set text(font: ("Cascadia Code", "Consolas", "DejaVu Sans Mono"), size: 0.9em)
#show raw.where(block: false): box.with(fill: rgb("#eff1f3"), inset: (x: 3pt), outset: (y: 3pt), radius: 3pt)
#show raw.where(block: true): block.with(fill: rgb("#f6f8fa"), inset: 10pt, radius: 4pt, width: 100%)
#show link: set text(fill: rgb("#0969da"))
#show quote.where(block: true): it => block(stroke: (left: 3pt + rgb("#d1d9e0")), inset: (left: 12pt, y: 4pt), text(fill: rgb("#59636e"), it.body))
#set table(stroke: 0.5pt + rgb("#d1d9e0"), inset: 6pt)
#show table.cell.where(y: 0): strong
#let md-image(path, alt) = layout(size => {
  let img = image(path, alt: alt)
  if measure(img).width > size.width { image(path, alt: alt, width: size.width) } else { img }
})
"##;

/// Typst source plus the image files it references (virtual path → disk file).
pub struct Converted {
    pub source: String,
    pub images: Vec<(String, LocalImage)>,
}

/// Converts Markdown to a complete Typst document.
pub fn convert(markdown: &str, title: &str, base_dir: Option<&Path>) -> Converted {
    let arena = Arena::new();
    let root = comrak::parse_document(&arena, markdown, &rmd_core::options());

    let mut writer = Writer {
        out: String::with_capacity(markdown.len() * 2),
        base_dir,
        images: Vec::new(),
        footnotes: HashMap::new(),
    };
    writer.collect_footnotes(root);
    writer.out.push_str(PREAMBLE);
    writeln!(writer.out, "#set document(title: {})", string(title)).unwrap();
    writer.blocks(root);
    Converted {
        source: writer.out,
        images: writer.images,
    }
}

/// Typst string literal.
fn string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

struct Writer<'p> {
    out: String,
    base_dir: Option<&'p Path>,
    images: Vec<(String, LocalImage)>,
    /// Footnote name → rendered body, inlined at the reference site.
    footnotes: HashMap<String, String>,
}

impl Writer<'_> {
    fn collect_footnotes<'a>(&mut self, root: &'a AstNode<'a>) {
        for node in root.descendants() {
            let name = match &node.data().value {
                NodeValue::FootnoteDefinition(def) => def.name.clone(),
                _ => continue,
            };
            let body = self.render_into(|w| w.children_inline_blocks(node));
            self.footnotes.insert(name, body);
        }
    }

    /// Runs `f` with an empty output buffer and returns what it wrote.
    fn render_into(&mut self, f: impl FnOnce(&mut Self)) -> String {
        let saved = std::mem::take(&mut self.out);
        f(self);
        std::mem::replace(&mut self.out, saved)
    }

    fn blocks<'a>(&mut self, node: &'a AstNode<'a>) {
        for child in node.children() {
            self.block(child);
        }
    }

    /// Renders the blocks of a container whose content sits inside `[...]`
    /// (list items, footnotes): paragraphs are separated, not terminated.
    fn children_inline_blocks<'a>(&mut self, node: &'a AstNode<'a>) {
        let mut first = true;
        for child in node.children() {
            if !first {
                self.out.push_str("\n\n");
            }
            first = false;
            if matches!(child.data().value, NodeValue::Paragraph) {
                self.inlines(child);
            } else {
                self.block(child);
            }
        }
    }

    fn block<'a>(&mut self, node: &'a AstNode<'a>) {
        let value = node.data().value.clone();
        match value {
            NodeValue::Paragraph => {
                self.inlines(node);
                self.out.push_str("\n\n");
            }
            NodeValue::Heading(heading) => {
                write!(self.out, "#heading(level: {})[", heading.level).unwrap();
                self.inlines(node);
                self.out.push_str("]\n\n");
            }
            NodeValue::CodeBlock(code) => {
                let lang = code.info.split_whitespace().next().unwrap_or("");
                let literal = code.literal.strip_suffix('\n').unwrap_or(&code.literal);
                if lang.is_empty() {
                    write!(self.out, "#raw(block: true, {})\n\n", string(literal)).unwrap();
                } else {
                    write!(
                        self.out,
                        "#raw(block: true, lang: {}, {})\n\n",
                        string(lang),
                        string(literal)
                    )
                    .unwrap();
                }
            }
            NodeValue::BlockQuote | NodeValue::MultilineBlockQuote(_) => {
                self.out.push_str("#quote(block: true)[");
                self.children_inline_blocks(node);
                self.out.push_str("]\n\n");
            }
            NodeValue::Alert(alert) => {
                let title = alert
                    .title
                    .clone()
                    .unwrap_or_else(|| alert.alert_type.default_title().to_owned());
                write!(
                    self.out,
                    "#quote(block: true)[#strong({})\n\n",
                    string(&title)
                )
                .unwrap();
                self.children_inline_blocks(node);
                self.out.push_str("]\n\n");
            }
            NodeValue::List(list) => {
                match list.list_type {
                    ListType::Bullet => self.out.push_str("#list(tight: "),
                    ListType::Ordered => {
                        write!(self.out, "#enum(start: {}, tight: ", list.start).unwrap()
                    }
                }
                self.out.push_str(if list.tight { "true" } else { "false" });
                for item in node.children() {
                    self.out.push_str(", [");
                    if let NodeValue::TaskItem(task) = item.data().value {
                        let mark = if task.symbol.is_some() { "☒" } else { "☐" };
                        write!(self.out, "#{} ", string(mark)).unwrap();
                    }
                    self.children_inline_blocks(item);
                    self.out.push(']');
                }
                self.out.push_str(")\n\n");
            }
            NodeValue::Table(table) => {
                let aligns: Vec<&str> = table
                    .alignments
                    .iter()
                    .map(|align| match align {
                        TableAlignment::Center => "center",
                        TableAlignment::Right => "right",
                        _ => "left",
                    })
                    .collect();
                write!(
                    self.out,
                    "#table(columns: {}, align: ({},)",
                    table.num_columns.max(1),
                    aligns.join(", ")
                )
                .unwrap();
                for row in node.children() {
                    let header = matches!(row.data().value, NodeValue::TableRow(true));
                    if header {
                        self.out.push_str(", table.header(");
                    }
                    for (i, cell) in row.children().enumerate() {
                        if !header || i > 0 {
                            self.out.push_str(", ");
                        }
                        self.out.push('[');
                        self.inlines(cell);
                        self.out.push(']');
                    }
                    if header {
                        self.out.push(')');
                    }
                }
                self.out.push_str(")\n\n");
            }
            NodeValue::ThematicBreak => {
                self.out
                    .push_str("#line(length: 100%, stroke: 0.6pt + rgb(\"#d1d9e0\"))\n\n");
            }
            // Raw HTML and front matter cannot be represented; footnote
            // definitions are inlined at their reference.
            NodeValue::HtmlBlock(_)
            | NodeValue::FrontMatter(_)
            | NodeValue::FootnoteDefinition(_) => {}
            _ => self.blocks(node),
        }
    }

    fn inlines<'a>(&mut self, node: &'a AstNode<'a>) {
        for child in node.children() {
            self.inline(child);
        }
    }

    fn wrapped<'a>(&mut self, function: &str, node: &'a AstNode<'a>) {
        write!(self.out, "#{function}[").unwrap();
        self.inlines(node);
        self.out.push(']');
    }

    fn inline<'a>(&mut self, node: &'a AstNode<'a>) {
        let value = node.data().value.clone();
        match value {
            NodeValue::Text(text) => {
                self.out.push('#');
                self.out.push_str(&string(&text));
            }
            NodeValue::SoftBreak => self.out.push_str("#\" \""),
            NodeValue::LineBreak => self.out.push_str("#linebreak()"),
            NodeValue::Code(code) => write!(self.out, "#raw({})", string(&code.literal)).unwrap(),
            NodeValue::Emph => self.wrapped("emph", node),
            NodeValue::Strong => self.wrapped("strong", node),
            NodeValue::Strikethrough => self.wrapped("strike", node),
            NodeValue::Link(link) => {
                if link.url.is_empty() || link.url.starts_with('#') {
                    self.inlines(node);
                } else {
                    write!(self.out, "#link({})[", string(&link.url)).unwrap();
                    self.inlines(node);
                    self.out.push(']');
                }
            }
            NodeValue::Image(link) => {
                let alt = rmd_core::collect_text(node);
                match LocalImage::resolve(&link.url, self.base_dir) {
                    Some(image) => {
                        let vpath = format!("/images/{}.{}", self.images.len(), image.extension);
                        write!(
                            self.out,
                            "#box(md-image({}, {}))",
                            string(&vpath),
                            string(&alt)
                        )
                        .unwrap();
                        self.images.push((vpath, image));
                    }
                    None => {
                        let label = if alt.is_empty() { &link.url } else { &alt };
                        write!(self.out, "#link({}, {})", string(&link.url), string(label))
                            .unwrap();
                    }
                }
            }
            NodeValue::FootnoteReference(reference) => {
                let body = self
                    .footnotes
                    .get(&reference.name)
                    .cloned()
                    .unwrap_or_default();
                write!(self.out, "#footnote[{body}]").unwrap();
            }
            NodeValue::HtmlInline(html) => {
                let tag = html.trim().to_ascii_lowercase();
                if tag.starts_with("<br") {
                    self.out.push_str("#linebreak()");
                }
            }
            _ => self.inlines(node),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(markdown: &str) -> String {
        let source = convert(markdown, "T", None).source;
        let start = source.find("#set document").unwrap();
        let body_start = source[start..].find('\n').unwrap() + start + 1;
        source[body_start..].trim().to_owned()
    }

    #[test]
    fn escapes_text_as_string_literals() {
        assert_eq!(body(r#"a "quote" \ #x"#), r##"#"a \"quote\" \\ #x""##);
    }

    #[test]
    fn converts_headings_and_emphasis() {
        assert_eq!(
            body("## Sub *it* **b**"),
            r##"#heading(level: 2)[#"Sub "#emph[#"it"]#" "#strong[#"b"]]"##
        );
    }

    #[test]
    fn converts_lists_and_tasks() {
        assert_eq!(
            body("- [x] done\n- [ ] todo"),
            r##"#list(tight: true, [#"☒" #"done"], [#"☐" #"todo"])"##
        );
        assert_eq!(
            body("3. a\n4. b"),
            r##"#enum(start: 3, tight: true, [#"a"], [#"b"])"##
        );
    }

    #[test]
    fn converts_code() {
        assert_eq!(
            body("```rust\nfn x() {}\n```"),
            r##"#raw(block: true, lang: "rust", "fn x() {}")"##
        );
        assert_eq!(body("`a\"b`"), r##"#raw("a\"b")"##);
    }

    #[test]
    fn converts_tables() {
        assert_eq!(
            body("| a | b |\n|:-:|--:|\n| 1 | 2 |"),
            r##"#table(columns: 2, align: (center, right,), table.header([#"a"], [#"b"]), [#"1"], [#"2"])"##
        );
    }

    #[test]
    fn inlines_footnotes() {
        assert_eq!(
            body("Text[^n].\n\n[^n]: Note."),
            r##"#"Text"#footnote[#"Note."]#".""##
        );
    }

    #[test]
    fn collects_local_images() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("p.png"), b"png").unwrap();
        let converted = convert("![alt](p.png)", "T", Some(dir.path()));
        assert!(
            converted
                .source
                .contains(r#"#box(md-image("/images/0.png", "alt"))"#)
        );
        assert_eq!(converted.images.len(), 1);
    }
}
