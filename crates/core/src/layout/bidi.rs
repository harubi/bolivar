//! Bidirectional text helpers for layout/text extraction.
//!
//! This module provides optional UAX#9 reordering for extracted text output.

use unicode_bidi::{BidiInfo, Level};
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Copy, PartialEq, Eq)]
enum WordBidiClass {
    Rtl,
    Ltr,
    Neutral,
}

fn contains_rtl_script(text: &str) -> bool {
    text.chars().any(|ch| {
        matches!(
            ch,
            '\u{0590}'..='\u{05FF}'
                | '\u{0600}'..='\u{06FF}'
                | '\u{0750}'..='\u{077F}'
                | '\u{0870}'..='\u{089F}'
                | '\u{08A0}'..='\u{08FF}'
                | '\u{FB1D}'..='\u{FDFF}'
                | '\u{FE70}'..='\u{FEFF}'
                | '\u{10E60}'..='\u{10E7F}'
                | '\u{1EE00}'..='\u{1EEFF}'
        )
    })
}

fn word_bidi_class(text: &str) -> WordBidiClass {
    if contains_rtl_script(text) {
        return WordBidiClass::Rtl;
    }
    if text.chars().any(char::is_alphabetic) {
        return WordBidiClass::Ltr;
    }
    WordBidiClass::Neutral
}

/// Reorder left-to-right geometric word runs into logical reading order.
pub(crate) fn reorder_visual_word_runs<T>(
    words: Vec<T>,
    text: impl for<'a> Fn(&'a T) -> &'a str,
) -> Vec<T> {
    if !words.iter().any(|word| contains_rtl_script(text(word))) {
        return words;
    }

    let mut runs: Vec<(WordBidiClass, Vec<T>)> = Vec::new();
    for word in words {
        let class = word_bidi_class(text(&word));
        if let Some((last_class, run)) = runs.last_mut()
            && *last_class == class
        {
            run.push(word);
            continue;
        }
        runs.push((class, vec![word]));
    }

    for (class, run) in &mut runs {
        if *class == WordBidiClass::Rtl {
            run.reverse();
        }
    }
    runs.reverse();

    runs.into_iter().flat_map(|(_, run)| run).collect()
}

/// Reorder bidirectional PDF text per line using UAX#9.
///
/// PDF layout input is geometrically ordered. RTL-containing lines force an RTL
/// paragraph level so mixed visual runs become logical extraction text while
/// newline structure remains unchanged.
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

fn contains_arabic_presentation_forms(text: &str) -> bool {
    text.chars()
        .any(|ch| matches!(ch, '\u{FB50}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}'))
}

/// Normalize Arabic presentation-form code points to their logical Unicode form.
///
/// This is intentionally narrow: only lines containing Arabic presentation
/// forms are normalized to avoid changing non-RTL text output.
pub fn normalize_presentation_forms_for_output(text: &str) -> String {
    if text.is_empty() || !contains_arabic_presentation_forms(text) {
        return text.to_string();
    }
    text.nfkc().collect()
}

/// Reorder a visual bidi token and normalize Arabic presentation forms.
///
/// This is the canonical string output path for user-facing text extraction.
pub fn reorder_text_for_output(text: &str) -> String {
    let reordered = reorder_text_per_line(text);
    normalize_presentation_forms_for_output(&reordered)
}

/// Convert geometrically ordered PDF text into logical reading order.
///
/// PDF layout elements are stored from left to right. Mixed-direction lines
/// therefore need their visual word runs reordered in addition to the
/// character-level UAX#9 conversion applied to each word.
pub fn reorder_visual_text_for_output(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    let mut out = String::with_capacity(text.len());
    for chunk in text.split_inclusive('\n') {
        let (line, has_newline) = match chunk.strip_suffix('\n') {
            Some(prefix) => (prefix, true),
            None => (chunk, false),
        };

        if contains_rtl_script(line) {
            let words = line
                .split_whitespace()
                .map(reorder_text_for_output)
                .collect::<Vec<_>>();
            let reordered = reorder_visual_word_runs(words, String::as_str);
            out.push_str(&reordered.join(" "));
        } else {
            out.push_str(line);
        }
        if has_newline {
            out.push('\n');
        }
    }
    out
}

fn reorder_single_line(line: &str) -> String {
    let paragraph_level = contains_rtl_script(line).then_some(Level::rtl());
    let info = BidiInfo::new(line, paragraph_level);
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

    #[test]
    fn arabic_visual_line_reorders_to_logical_and_keeps_digits() {
        let line = "1120280977 :ﻊﺟﺮﻤﻟﺍ ﻢﻗﺭ";
        assert_eq!(reorder_text_per_line(line), "ﺭﻗﻢ ﺍﻟﻤﺮﺟﻊ: 1120280977");
    }

    #[test]
    fn arabic_visual_words_reorder_to_logical() {
        let line = "ﺏﺎﺴﺤﻟﺍ ﻒﺸﻛ";
        assert_eq!(reorder_text_per_line(line), "ﻛﺸﻒ ﺍﻟﺤﺴﺎﺏ");
    }

    #[test]
    fn arabic_presentation_forms_normalize_to_logical_unicode_output() {
        let line = "ﺏﺎﺴﺤﻟﺍ ﻒﺸﻛ";
        assert_eq!(reorder_text_for_output(line), "كشف الحساب");
    }

    #[test]
    fn hebrew_visual_line_reorders_to_logical_and_keeps_digits() {
        let line = "1120280977 :םולש";
        assert_eq!(reorder_text_per_line(line), "שלום: 1120280977");
    }

    #[test]
    fn urdu_visual_line_reorders_to_logical_and_keeps_digits() {
        let line = "1120280977 :ہلاوح ربمن";
        assert_eq!(reorder_text_per_line(line), "نمبر حوالہ: 1120280977");
    }
}
