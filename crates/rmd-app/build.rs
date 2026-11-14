//! Builds the application icon from the Caretta design system SVG.
//!
//! `assets/icon.svg` is the single source: it is rendered at every size
//! Windows uses into `icon.ico` (embedded in the executable, shown by
//! Explorer and the taskbar) and into a 256 px PNG for the window icon.

use std::path::PathBuf;

use resvg::{tiny_skia, usvg};

const ICON_SVG: &str = "assets/icon.svg";
/// Sizes Windows picks from (small icons, lists, tiles, high DPI).
const ICO_SIZES: &[u32] = &[16, 20, 24, 32, 40, 48, 64, 128, 256];

fn main() {
    println!("cargo:rerun-if-changed={ICON_SVG}");
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));

    let svg = std::fs::read(ICON_SVG).expect("read the icon SVG");
    let tree = usvg::Tree::from_data(&svg, &usvg::Options::default()).expect("valid icon SVG");

    let mut icon_dir = ico::IconDir::new(ico::ResourceType::Icon);
    for &size in ICO_SIZES {
        let pixmap = render(&tree, size);
        // ICO wants straight (non-premultiplied) RGBA.
        let rgba: Vec<u8> = pixmap
            .pixels()
            .iter()
            .flat_map(|pixel| {
                let color = pixel.demultiply();
                [color.red(), color.green(), color.blue(), color.alpha()]
            })
            .collect();
        let image = ico::IconImage::from_rgba_data(size, size, rgba);
        icon_dir.add_entry(ico::IconDirEntry::encode(&image).expect("encode icon entry"));
        if size == 256 {
            let png = pixmap.encode_png().expect("encode window icon");
            std::fs::write(out_dir.join("icon-256.png"), png).expect("write window icon");
        }
    }
    let ico_path = out_dir.join("icon.ico");
    let file = std::fs::File::create(&ico_path).expect("create icon.ico");
    icon_dir.write(file).expect("write icon.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon(ico_path.to_str().expect("UTF-8 OUT_DIR"))
            .set("ProductName", "Rust Markdown")
            .set(
                "FileDescription",
                "Rust Markdown — lecteur et éditeur Markdown",
            );
        resource.compile().expect("compile Windows resources");
    }
}

fn render(tree: &usvg::Tree, size: u32) -> tiny_skia::Pixmap {
    let mut pixmap = tiny_skia::Pixmap::new(size, size).expect("non-zero icon size");
    let scale = size as f32 / tree.size().width().max(tree.size().height());
    resvg::render(
        tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
}
