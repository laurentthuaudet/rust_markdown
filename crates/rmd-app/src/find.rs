//! Text search and replacement in the editor content (pure functions).

use std::ops::Range;

/// Non-overlapping occurrences of `query` in `text`, as byte ranges.
///
/// Case-insensitive matching compares characters through their lowercase
/// form, so "É" matches "é".
pub fn find_all(text: &str, query: &str, case_sensitive: bool) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    let fold = |c: char| {
        if case_sensitive {
            c
        } else {
            c.to_lowercase().next().unwrap_or(c)
        }
    };
    let needle: Vec<char> = query.chars().map(fold).collect();
    let haystack: Vec<(usize, char)> = text.char_indices().map(|(i, c)| (i, fold(c))).collect();

    let mut matches = Vec::new();
    let mut i = 0;
    while i + needle.len() <= haystack.len() {
        let window = &haystack[i..i + needle.len()];
        if window.iter().map(|&(_, c)| c).eq(needle.iter().copied()) {
            let start = haystack[i].0;
            let end = haystack
                .get(i + needle.len())
                .map_or(text.len(), |&(index, _)| index);
            matches.push(start..end);
            i += needle.len();
        } else {
            i += 1;
        }
    }
    matches
}

/// Converts sorted byte ranges to UTF-16 offsets (as used by the web view),
/// in a single pass over the text.
pub fn to_utf16(text: &str, ranges: &[Range<usize>]) -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(ranges.len());
    let (mut byte, mut unit) = (0, 0);
    let mut advance = |to: usize| {
        unit += text[byte..to].encode_utf16().count();
        byte = to;
        unit
    };
    for range in ranges {
        let start = advance(range.start);
        let end = advance(range.end);
        out.push((start, end));
    }
    out
}

/// Replaces one occurrence.
pub fn replace_one(text: &str, range: &Range<usize>, replacement: &str) -> String {
    let mut out = String::with_capacity(text.len() + replacement.len());
    out.push_str(&text[..range.start]);
    out.push_str(replacement);
    out.push_str(&text[range.end..]);
    out
}

/// Replaces every occurrence; returns the new text and the count.
pub fn replace_all(
    text: &str,
    query: &str,
    replacement: &str,
    case_sensitive: bool,
) -> (String, usize) {
    let matches = find_all(text, query, case_sensitive);
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for range in &matches {
        out.push_str(&text[last..range.start]);
        out.push_str(replacement);
        last = range.end;
    }
    out.push_str(&text[last..]);
    (out, matches.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_case_insensitively_by_default() {
        let text = "Été, été et ÉTÉ";
        assert_eq!(find_all(text, "été", false).len(), 3);
        assert_eq!(find_all(text, "été", true).len(), 1);
        assert!(find_all(text, "", false).is_empty());
    }

    #[test]
    fn matches_do_not_overlap() {
        assert_eq!(find_all("aaaa", "aa", true), [0..2, 2..4]);
    }

    #[test]
    fn converts_to_utf16_offsets() {
        let text = "😀 é x é";
        let ranges = find_all(text, "é", true);
        // The emoji takes two UTF-16 code units.
        assert_eq!(to_utf16(text, &ranges), [(3, 4), (7, 8)]);
    }

    #[test]
    fn replaces_one_and_all() {
        let text = "un deux un";
        let matches = find_all(text, "un", true);
        assert_eq!(replace_one(text, &matches[1], "1"), "un deux 1");
        assert_eq!(
            replace_all(text, "UN", "1", false),
            ("1 deux 1".to_owned(), 2)
        );
    }
}
