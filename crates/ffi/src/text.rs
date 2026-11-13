//! Text rules the JVM API has always followed.

use std::sync::LazyLock;

use regex::Regex;

/// Whether every code point of `text` is whitespace by Java's
/// `Character.isWhitespace`: a Unicode space, line or paragraph separator
/// other than the no-break spaces U+00A0, U+2007 and U+202F, or one of
/// U+0009 to U+000D and U+001C to U+001F. Empty text is blank.
pub fn java_blank(text: &str) -> bool {
    text.chars().all(|character| {
        matches!(
            character,
            '\u{0009}'..='\u{000D}'
                | '\u{001C}'..='\u{0020}'
                | '\u{1680}'
                | '\u{2000}'..='\u{2006}'
                | '\u{2008}'..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{205F}'
                | '\u{3000}'
        )
    })
}

static DECIMAL_DIGIT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\p{Nd}").expect("the decimal digit class compiles"));

/// Whether `text` holds a decimal digit (Unicode category Nd), as Java's
/// `\p{Nd}` finds one.
pub fn has_decimal_digit(text: &str) -> bool {
    if text.is_ascii() {
        text.bytes().any(|byte| byte.is_ascii_digit())
    } else {
        DECIMAL_DIGIT.is_match(text)
    }
}

/// A size rounded as Java's `Math.round` rounds it: the nearest whole
/// number, halves toward positive infinity, NaN to zero, saturated at the
/// `i32` range.
pub fn java_round(size: f64) -> i32 {
    let floor = size.floor();
    let rounded = if size - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    };
    // `as` saturates out-of-range values and maps NaN to zero.
    rounded as i32
}

/// A layout page id as a 1-based page number; non-positive ids become 1.
pub fn page_number(pageid: i32) -> i64 {
    match u32::try_from(pageid) {
        Ok(0) | Err(_) => 1,
        Ok(page_number) => i64::from(page_number),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nbsp_is_not_blank() {
        for space in ['\u{00A0}', '\u{2007}', '\u{202F}'] {
            assert!(!java_blank(&space.to_string()), "{space:?}");
        }
        for space in [
            "",
            " ",
            "\t",
            "\n",
            "\u{000B}",
            "\u{000C}",
            "\r",
            "\u{001C}",
            "\u{001F}",
            "\u{1680}",
            "\u{2000}",
            "\u{2006}",
            "\u{2008}",
            "\u{200A}",
            "\u{2028}",
            "\u{2029}",
            "\u{205F}",
            "\u{3000}",
            " \u{3000}\t",
        ] {
            assert!(java_blank(space), "{space:?}");
        }
        for text in ["\u{0085}", "\u{180E}", "\u{200B}", "a", " a"] {
            assert!(!java_blank(text), "{text:?}");
        }
    }

    #[test]
    fn arabic_indic_digit_is_a_digit() {
        for digit in '\u{0660}'..='\u{0669}' {
            assert!(has_decimal_digit(&digit.to_string()), "{digit:?}");
        }
        assert!(has_decimal_digit("\u{06F5}"));
        assert!(has_decimal_digit("a7"));
        assert!(has_decimal_digit("ب٣"));
        assert!(!has_decimal_digit("abc"));
        assert!(
            !has_decimal_digit("\u{00BD}"),
            "a vulgar fraction is No, not Nd"
        );
        assert!(
            !has_decimal_digit("\u{2163}"),
            "a roman numeral is Nl, not Nd"
        );
    }

    #[test]
    fn size_rounds_half_up() {
        assert_eq!(java_round(8.5), 9);
        assert_eq!(java_round(-0.5), 0);
        assert_eq!(java_round(8.4), 8);
        assert_eq!(java_round(-1.5), -1);
        assert_eq!(java_round(-1.6), -2);
        assert_eq!(java_round(0.499_999_999_999_999_94), 0);
        assert_eq!(java_round(f64::NAN), 0);
        assert_eq!(java_round(1e12), i32::MAX);
    }

    #[test]
    fn page_number_clamps_non_positive_to_one() {
        assert_eq!(page_number(0), 1);
        assert_eq!(page_number(-5), 1);
        assert_eq!(page_number(7), 7);
    }
}
