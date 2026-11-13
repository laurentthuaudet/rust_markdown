//! Serves local images to the preview web view.
//!
//! The web view cannot load `C:\...` paths directly, so image sources are
//! rewritten to a custom protocol (`http://localfile.localhost/<encoded path>`
//! on Windows) that this module answers. Only image files are served.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dioxus::desktop::wry::WebViewId;
use dioxus::desktop::wry::http::{Request, Response, StatusCode, header};
use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use rmd_core::UrlRewriter;

/// Name of the custom protocol registered with the web view.
pub const SCHEME: &str = "localfile";

/// Builds an image URL rewriter resolving relative paths against `base_dir`.
pub fn image_rewriter(base_dir: Option<PathBuf>) -> UrlRewriter {
    Arc::new(move |url: &str| rewrite(url, base_dir.as_deref()))
}

fn rewrite(url: &str, base_dir: Option<&Path>) -> String {
    let is_remote = url.contains("://") || url.starts_with("data:") || url.starts_with('#');
    if is_remote || url.is_empty() {
        return url.to_owned();
    }
    let raw = url.split(['?', '#']).next().unwrap_or(url);
    let decoded = percent_decode_str(raw).decode_utf8_lossy();
    let path = Path::new(decoded.as_ref());
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else if let Some(base) = base_dir {
        base.join(path)
    } else {
        return url.to_owned();
    };
    let encoded = utf8_percent_encode(&absolute.to_string_lossy(), NON_ALPHANUMERIC).to_string();
    format!("http://{SCHEME}.localhost/{encoded}")
}

fn mime_type(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "avif" => "image/avif",
        _ => return None,
    })
}

/// Custom protocol handler: returns the requested image file.
pub fn handle(_id: WebViewId, request: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    let encoded = request.uri().path().trim_start_matches('/');
    let path = PathBuf::from(percent_decode_str(encoded).decode_utf8_lossy().as_ref());

    let response = match mime_type(&path) {
        None => Err(StatusCode::FORBIDDEN),
        Some(mime) => std::fs::read(&path)
            .map(|bytes| (mime, bytes))
            .map_err(|_| StatusCode::NOT_FOUND),
    };
    match response {
        Ok((mime, bytes)) => Response::builder()
            .header(header::CONTENT_TYPE, mime)
            .body(Cow::Owned(bytes)),
        Err(status) => Response::builder()
            .status(status)
            .body(Cow::Borrowed(&[][..])),
    }
    .expect("static response parts are valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_remote_urls() {
        let base = Some(Path::new(r"C:\docs"));
        for url in [
            "https://x.org/a.png",
            "data:image/png;base64,AA",
            "#top",
            "",
        ] {
            assert_eq!(rewrite(url, base), url);
        }
    }

    #[test]
    fn resolves_relative_paths() {
        let url = rewrite("img/a%20b.png", Some(Path::new(r"C:\docs")));
        assert_eq!(
            url,
            "http://localfile.localhost/C%3A%5Cdocs%5Cimg%2Fa%20b%2Epng"
        );
    }

    #[test]
    fn leaves_relative_paths_without_base() {
        assert_eq!(rewrite("a.png", None), "a.png");
    }

    #[test]
    fn serves_only_images() {
        assert_eq!(mime_type(Path::new("a.PNG")), Some("image/png"));
        assert_eq!(mime_type(Path::new("secret.txt")), None);
    }
}
