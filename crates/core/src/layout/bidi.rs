//! Bidirectional text helpers for layout/text extraction.
//!
//! This module provides optional UAX#9 reordering for extracted text output.

use unicode_bidi::BidiInfo;

/// Reorder bidirectional text per line (split on `\n`) using UAX#9.
///
/// This keeps newline structure stable while converting each logical line
/// to visual order for plain-text output.
pub fn reorder_text_per_line(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    let mut out = String::with_capacity(text.len());
    for chunk in text.split_inclusive('\n') {
        let (line, has_newline) = match chunk.strip_suffix('\n') {
            Some(prefix) => (prefix, true),
            None => (chunk, false),
        };

        if !line.is_empty() {
            out.push_str(&reorder_single_line(line));
        }
        if has_newline {
            out.push('\n');
        }
    }
    out
}

fn reorder_single_line(line: &str) -> String {
    let info = BidiInfo::new(line, None);
    if info.paragraphs.is_empty() {
        return line.to_string();
    }

    let mut out = String::with_capacity(line.len());
    for para in &info.paragraphs {
        out.push_str(&info.reorder_line(para, para.range.clone()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_stays_empty() {
        assert_eq!(reorder_text_per_line(""), "");
    }

    #[test]
    fn ltr_text_is_unchanged() {
        assert_eq!(reorder_text_per_line("abc 123"), "abc 123");
    }

    #[test]
    fn preserves_newline_boundaries() {
        let got = reorder_text_per_line("abc\n\u{05D0}\u{05D1}\u{05D2}\n");
        assert!(got.starts_with("abc\n"));
        assert!(got.ends_with('\n'));
    }
}
