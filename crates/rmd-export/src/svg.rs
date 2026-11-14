//! SVG rasterisation, for formats that only accept bitmap images (DOCX).

use std::path::Path;
use std::sync::{Arc, LazyLock};

use resvg::{tiny_skia, usvg};

/// Rendering density: SVGs are drawn at 2× their size so they stay sharp
/// when printed or zoomed.
const SCALE: f32 = 2.0;
/// Upper bound on the bitmap side, to keep documents reasonably small.
const MAX_PIXELS: f32 = 4096.0;

/// System fonts, for SVG text. Loading them is slow, so it is done once.
static FONTS: LazyLock<Arc<usvg::fontdb::Database>> = LazyLock::new(|| {
    let mut fonts = usvg::fontdb::Database::new();
    fonts.load_system_fonts();
    Arc::new(fonts)
});

pub struct Rasterized {
    pub png: Vec<u8>,
    /// Bitmap size in pixels.
    pub pixel_width: u32,
    pub pixel_height: u32,
    /// Display size in CSS pixels (the SVG's own size).
    pub width: u32,
    pub height: u32,
}

/// Renders an SVG document to PNG. `resources_dir` resolves images the SVG
/// references by relative path. Returns `None` for invalid or empty SVGs.
pub fn rasterize(data: &[u8], resources_dir: Option<&Path>) -> Option<Rasterized> {
    let options = usvg::Options {
        resources_dir: resources_dir.map(Path::to_path_buf),
        fontdb: FONTS.clone(),
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_data(data, &options).ok()?;
    let size = tree.size();
    let scale = SCALE.min(MAX_PIXELS / size.width().max(size.height()));
    let pixel_width = (size.width() * scale).ceil() as u32;
    let pixel_height = (size.height() * scale).ceil() as u32;
    let mut pixmap = tiny_skia::Pixmap::new(pixel_width, pixel_height)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(Rasterized {
        png: pixmap.encode_png().ok()?,
        pixel_width,
        pixel_height,
        width: size.width().ceil().max(1.0) as u32,
        height: size.height().ceil().max(1.0) as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20">
        <rect width="40" height="20" fill="#d33"/></svg>"##;

    #[test]
    fn renders_at_double_density() {
        let image = rasterize(SVG.as_bytes(), None).unwrap();
        assert_eq!((image.width, image.height), (40, 20));
        assert_eq!((image.pixel_width, image.pixel_height), (80, 40));
        assert!(image.png.starts_with(b"\x89PNG"));
        let decoded = image::load_from_memory(&image.png).unwrap();
        // Centre pixel is the rectangle's red.
        let pixel = decoded.to_rgba8().get_pixel(40, 20).0;
        assert_eq!(pixel, [0xdd, 0x33, 0x33, 0xff]);
    }

    #[test]
    fn caps_huge_drawings() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10000" height="100"/>"#;
        let image = rasterize(svg.as_bytes(), None).unwrap();
        assert_eq!(image.pixel_width, 4096);
    }

    #[test]
    fn rejects_invalid_svg() {
        assert!(rasterize(b"not an svg", None).is_none());
    }
}
