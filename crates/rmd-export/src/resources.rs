//! Resolution of local images referenced by a document.

use std::path::{Path, PathBuf};

/// An image file found on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalImage {
    pub path: PathBuf,
    pub mime: &'static str,
    pub extension: &'static str,
}

impl LocalImage {
    /// Resolves `url` to an existing local image, if it is one.
    ///
    /// Remote URLs (`http://`, `data:`…) and missing files yield `None`.
    pub fn resolve(url: &str, base_dir: Option<&Path>) -> Option<Self> {
        if url.is_empty() || url.contains("://") || url.starts_with("data:") {
            return None;
        }
        let raw = url.split(['?', '#']).next().unwrap_or(url);
        let decoded = percent_decode(raw);
        let path = Path::new(&decoded);
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            base_dir?.join(path)
        };
        let (mime, extension) = image_type(&path)?;
        path.is_file().then_some(Self {
            path,
            mime,
            extension,
        })
    }

    pub fn read(&self) -> std::io::Result<Vec<u8>> {
        std::fs::read(&self.path)
    }
}

fn image_type(path: &Path) -> Option<(&'static str, &'static str)> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => ("image/png", "png"),
        "jpg" | "jpeg" => ("image/jpeg", "jpg"),
        "gif" => ("image/gif", "gif"),
        "svg" => ("image/svg+xml", "svg"),
        "webp" => ("image/webp", "webp"),
        _ => return None,
    })
}

/// Minimal percent-decoding (`%20` → space) for Markdown link destinations.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let (Some(hi), Some(lo)) = (hex(bytes.get(i + 1)), hex(bytes.get(i + 2)))
        {
            out.push(hi << 4 | lo);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(byte: Option<&u8>) -> Option<u8> {
    (*byte? as char).to_digit(16).map(|digit| digit as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_existing_relative_images() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a b.png"), b"png").unwrap();

        let image = LocalImage::resolve("a%20b.png", Some(dir.path())).unwrap();
        assert_eq!(image.path, dir.path().join("a b.png"));
        assert_eq!(image.mime, "image/png");

        assert!(LocalImage::resolve("missing.png", Some(dir.path())).is_none());
        assert!(LocalImage::resolve("https://x.org/a.png", Some(dir.path())).is_none());
        assert!(LocalImage::resolve("a.png", None).is_none());
    }

    #[test]
    fn decodes_percent_escapes() {
        assert_eq!(percent_decode("a%20b%C3%A9"), "a bé");
        assert_eq!(percent_decode("100%"), "100%");
    }
}
