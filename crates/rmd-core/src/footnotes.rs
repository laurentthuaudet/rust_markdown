//! Renders footnote definitions where the author wrote them.
//!
//! GFM moves every footnote to the end of the document. We parse with
//! `leave_footnote_definitions` so definitions stay in place, but comrak's
//! HTML formatter then opens its `<section class="footnotes">` at the first
//! definition and only closes it at the end of the document, swallowing
//! everything in between. So each run of consecutive definitions is rendered
//! here as its own section and spliced back into the tree as an HTML block.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Write;

use comrak::Options;
use comrak::nodes::{Ast, AstNode, NodeHtmlBlock, NodeValue};
use comrak::options::Plugins;

/// Replaces footnote definitions under `root` by in-place HTML sections.
pub fn place_in_place<'a>(
    arena: &'a comrak::Arena<'a>,
    root: &'a AstNode<'a>,
    options: &Options,
    plugins: &Plugins,
) {
    // Footnote numbers follow the order of first reference, as on GitHub.
    let mut numbers: HashMap<String, u32> = HashMap::new();
    for node in root.descendants() {
        if let NodeValue::FootnoteReference(reference) = &node.data().value {
            numbers
                .entry(reference.name.clone())
                .or_insert(reference.ix);
        }
    }

    let definitions: Vec<&AstNode<'_>> = root
        .descendants()
        .filter(|node| is_definition(node))
        .collect();
    let mut runs: Vec<Vec<&AstNode<'_>>> = Vec::new();
    for definition in definitions {
        let continues_run = definition.previous_sibling().is_some_and(is_definition);
        match runs.last_mut() {
            Some(run) if continues_run => run.push(definition),
            _ => runs.push(vec![definition]),
        }
    }

    for run in runs {
        let mut items = String::new();
        for definition in &run {
            let (name, references) = match &definition.data().value {
                NodeValue::FootnoteDefinition(def) => (def.name.clone(), def.total_references),
                _ => continue,
            };
            // Unreferenced definitions are not displayed (same as comrak).
            if let Some(&number) = numbers.get(&name) {
                items.push_str(&render_item(
                    definition, &name, number, references, options, plugins,
                ));
            }
        }
        let html = if items.is_empty() {
            String::new()
        } else {
            format!(
                "<section class=\"footnotes\" data-footnotes=\"\">\n<ol>\n{items}</ol>\n</section>\n"
            )
        };
        let sourcepos = run[0].data().sourcepos;
        let block = arena.alloc(AstNode::new(RefCell::new(Ast::new_with_sourcepos(
            NodeValue::HtmlBlock(NodeHtmlBlock {
                block_type: 6,
                literal: html,
            }),
            sourcepos,
        ))));
        run[0].insert_before(block);
        for definition in run {
            definition.detach();
        }
    }
}

fn is_definition(node: &AstNode<'_>) -> bool {
    matches!(node.data().value, NodeValue::FootnoteDefinition(_))
}

fn render_item<'a>(
    definition: &'a AstNode<'a>,
    name: &str,
    number: u32,
    references: u32,
    options: &Options,
    plugins: &Plugins,
) -> String {
    let id = attribute(name);
    let mut body = String::new();
    for child in definition.children() {
        // Formatting a subtree cannot fail when writing to a String.
        let _ = comrak::format_html_with_plugins(child, options, &mut body, plugins);
    }

    let mut backrefs = String::new();
    for reference in 1..=references.max(1) {
        let suffix = if reference > 1 {
            format!("-{reference}")
        } else {
            String::new()
        };
        let superscript = if reference > 1 {
            format!("<sup class=\"footnote-ref\">{reference}</sup>")
        } else {
            String::new()
        };
        let _ = write!(
            backrefs,
            " <a href=\"#fnref-{id}{suffix}\" class=\"footnote-backref\" \
             aria-label=\"Retour à la référence {number}{suffix}\">↩{superscript}</a>"
        );
    }

    // Put the back-links at the end of the last paragraph, like GitHub.
    let body = body.trim_end();
    let body = match body.strip_suffix("</p>") {
        Some(start) => format!("{}{backrefs}</p>", start.trim_end()),
        None => format!("{body}{backrefs}"),
    };
    format!("<li id=\"fn-{id}\" value=\"{number}\">\n{body}\n</li>\n")
}

/// Escapes a footnote name for use in an attribute, like comrak's anchors.
fn attribute(name: &str) -> String {
    let mut out = String::new();
    let _ = comrak::html::escape_href(&mut out, name, false);
    out
}

#[cfg(test)]
mod tests {
    use crate::render_html;

    #[test]
    fn keeps_definitions_where_written() {
        let html = render_html("Texte[^n].\n\n### Notes\n[^n]: La note.\n\n## Après\n\nFin.");
        let notes = html.find("id=\"fn-n\"").unwrap();
        let after = html.find("<h2").unwrap();
        assert!(notes < after, "{html}");
        // The section is closed before the following heading.
        assert!(html.find("</section>").unwrap() < after, "{html}");
        assert!(html.contains("La note. <a href=\"#fnref-n\""), "{html}");
    }

    #[test]
    fn numbers_by_first_reference_and_links_every_reference() {
        let html = render_html("a[^b] b[^a] c[^b]\n\n[^a]: A\n[^b]: B");
        assert!(html.contains("<li id=\"fn-a\" value=\"2\">"), "{html}");
        assert!(html.contains("<li id=\"fn-b\" value=\"1\">"), "{html}");
        assert!(html.contains("href=\"#fnref-b-2\""), "{html}");
    }

    #[test]
    fn separate_runs_get_separate_sections() {
        let html = render_html("x[^1] y[^2]\n\n[^1]: un\n\nmilieu\n\n[^2]: deux");
        assert_eq!(
            html.matches("<section class=\"footnotes\"").count(),
            2,
            "{html}"
        );
        let middle = html.find("milieu").unwrap();
        assert!(html.find("fn-1").unwrap() < middle && middle < html.find("id=\"fn-2\"").unwrap());
    }

    #[test]
    fn drops_unreferenced_definitions() {
        let html = render_html("texte\n\n[^x]: orpheline");
        assert!(!html.contains("orpheline"), "{html}");
    }
}
