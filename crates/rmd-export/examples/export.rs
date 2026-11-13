//! Exports a Markdown file from the command line (handy to check exporters).
//!
//! ```text
//! cargo run -p rmd-export --example export -- input.md output.{html,pdf,docx,epub}
//! ```

use std::path::PathBuf;

use rmd_export::{Document, Format};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1).map(PathBuf::from);
    let (Some(input), Some(output)) = (args.next(), args.next()) else {
        return Err("usage: export <input.md> <output.{html,pdf,docx,epub}>".into());
    };
    let extension = output.extension().and_then(|e| e.to_str()).unwrap_or("");
    let format = Format::ALL
        .iter()
        .copied()
        .find(|format| format.extension() == extension)
        .ok_or_else(|| format!("unsupported output extension: {extension:?}"))?;

    let markdown = std::fs::read_to_string(&input)?;
    let doc = Document::new(&markdown, Some(&input));
    rmd_export::export_to_file(&doc, format, &output)?;
    println!("{} written", output.display());
    Ok(())
}
