//! Caretta typefaces embedded in the application, so the interface looks the
//! same offline and never contacts a font service.
//!
//! Fraunces, Instrument Sans and JetBrains Mono (SIL Open Font License, see
//! `assets/fonts/OFL-*.txt`), as variable WOFF2 files split into the Latin and
//! Latin Extended subsets served by Google Fonts. They are inlined as data
//! URIs in `@font-face` rules; the browser only decodes the subsets it needs.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

const LATIN: &str = "U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, U+02DC, \
    U+0304, U+0308, U+0329, U+2000-206F, U+20AC, U+2122, U+2191, U+2193, U+2212, U+2215, \
    U+FEFF, U+FFFD";
const LATIN_EXT: &str = "U+0100-02BA, U+02BD-02C5, U+02C7-02CC, U+02CE-02D7, U+02DD-02FF, \
    U+0304, U+0308, U+0329, U+1D00-1DBF, U+1E00-1E9F, U+1EF2-1EFF, U+2020, U+20A0-20AB, \
    U+20AD-20C4, U+2113, U+2C60-2C7F, U+A720-A7FF";

struct Face {
    family: &'static str,
    /// Weight range covered by the variable font.
    weight: &'static str,
    unicode_range: &'static str,
    woff2: &'static [u8],
}

macro_rules! font {
    ($file:literal) => {
        include_bytes!(concat!("../assets/fonts/", $file))
    };
}

const FACES: &[Face] = &[
    Face {
        family: "Fraunces",
        weight: "400 600",
        unicode_range: LATIN,
        woff2: font!("Fraunces-normal-latin.woff2"),
    },
    Face {
        family: "Fraunces",
        weight: "400 600",
        unicode_range: LATIN_EXT,
        woff2: font!("Fraunces-normal-latin-ext.woff2"),
    },
    Face {
        family: "Instrument Sans",
        weight: "400 600",
        unicode_range: LATIN,
        woff2: font!("InstrumentSans-normal-latin.woff2"),
    },
    Face {
        family: "Instrument Sans",
        weight: "400 600",
        unicode_range: LATIN_EXT,
        woff2: font!("InstrumentSans-normal-latin-ext.woff2"),
    },
    Face {
        family: "JetBrains Mono",
        weight: "400",
        unicode_range: LATIN,
        woff2: font!("JetBrainsMono-normal-latin.woff2"),
    },
    Face {
        family: "JetBrains Mono",
        weight: "400",
        unicode_range: LATIN_EXT,
        woff2: font!("JetBrainsMono-normal-latin-ext.woff2"),
    },
];

/// `@font-face` rules for every embedded face.
pub fn font_face_css() -> String {
    FACES
        .iter()
        .map(|face| {
            format!(
                "@font-face {{ font-family: '{}'; font-style: normal; font-weight: {}; \
                 font-display: block; src: url(data:font/woff2;base64,{}) format('woff2'); \
                 unicode-range: {}; }}\n",
                face.family,
                face.weight,
                STANDARD.encode(face.woff2),
                face.unicode_range,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_every_face_as_woff2() {
        for face in FACES {
            assert!(
                face.woff2.starts_with(b"wOF2"),
                "{} is not WOFF2",
                face.family
            );
        }
        let css = font_face_css();
        assert_eq!(css.matches("@font-face").count(), FACES.len());
        for family in ["Fraunces", "Instrument Sans", "JetBrains Mono"] {
            assert!(css.contains(&format!("font-family: '{family}'")));
        }
        assert!(!css.contains("googleapis"));
    }
}
