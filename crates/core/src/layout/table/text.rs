//! Text extraction and word formation for table cells.
//!
//! This module handles converting characters to words and extracting
//! text from table cells with proper text direction handling.

use std::collections::HashMap;

use super::clustering::{bbox_from_chars, cluster_objects};
use super::types::{CharId, CharObj, TextDir, TextSettings, WordObj};

/// Get the line cluster key for a word based on text direction.
pub(crate) fn get_line_cluster_key(dir: TextDir, obj: &WordObj) -> f64 {
    match dir {
        TextDir::Ttb => obj.top,
        TextDir::Btt => -obj.bottom,
        TextDir::Ltr => obj.x0,
        TextDir::Rtl => -obj.x1,
    }
}

/// Get the character sort key based on text direction.
fn get_char_sort_key(dir: TextDir, obj: &CharObj) -> (f64, f64) {
    match dir {
        TextDir::Ttb => (obj.top, obj.bottom),
        TextDir::Btt => (-(obj.top + obj.height), -obj.top),
        TextDir::Ltr => (obj.x0, obj.x0),
        TextDir::Rtl => (-obj.x1, -obj.x0),
    }
}

/// Get the character direction based on upright status and settings.
fn get_char_dir(upright: bool, settings: &TextSettings) -> TextDir {
    if !upright && !settings.vertical_ttb {
        return TextDir::Btt;
    }
    if upright && !settings.horizontal_ltr {
        return TextDir::Rtl;
    }
    if upright {
        settings.char_dir
    } else {
        settings.char_dir_rotated.unwrap_or(settings.line_dir)
    }
}

/// Merge characters into a word.
pub(crate) fn merge_chars(ordered: &[&CharObj], settings: &TextSettings) -> WordObj {
    let bbox = bbox_from_chars(ordered);
    let doctop_adj = ordered[0].doctop - ordered[0].top;
    let upright = ordered[0].upright;
    let char_dir = get_char_dir(upright, settings);

    let text = ordered
        .iter()
        .map(|c| expand_ligature(&c.text, settings.expand_ligatures))
        .collect::<String>();

    WordObj {
        text,
        x0: bbox.x0,
        x1: bbox.x1,
        top: bbox.top,
        bottom: bbox.bottom,
        doctop: bbox.top + doctop_adj,
        height: bbox.height(),
        width: bbox.width(),
        upright,
        direction: char_dir,
    }
}

/// Expand ligature characters to their component characters.
fn expand_ligature(text: &str, expand: bool) -> String {
    if !expand {
        return text.to_string();
    }
    match text {
        "\u{fb00}" => "ff".to_string(),
        "\u{fb03}" => "ffi".to_string(),
        "\u{fb04}" => "ffl".to_string(),
        "\u{fb01}" => "fi".to_string(),
        "\u{fb02}" => "fl".to_string(),
        "\u{fb06}" => "st".to_string(),
        "\u{fb05}" => "st".to_string(),
        _ => text.to_string(),
    }
}

/// Check if a character begins a new word.
fn char_begins_new_word(
    prev: &CharObj,
    curr: &CharObj,
    direction: TextDir,
    x_tolerance: f64,
    y_tolerance: f64,
) -> bool {
    let (x, y, ay, cy, ax, bx, cx) = match direction {
        TextDir::Ltr => (
            x_tolerance,
            y_tolerance,
            prev.top,
            curr.top,
            prev.x0,
            prev.x1,
            curr.x0,
        ),
        TextDir::Rtl => (
            x_tolerance,
            y_tolerance,
            prev.top,
            curr.top,
            -prev.x1,
            -prev.x0,
            -curr.x1,
        ),
        TextDir::Ttb => (
            y_tolerance,
            x_tolerance,
            prev.x0,
            curr.x0,
            prev.top,
            prev.bottom,
            curr.top,
        ),
        TextDir::Btt => (
            y_tolerance,
            x_tolerance,
            prev.x0,
            curr.x0,
            -prev.bottom,
            -prev.top,
            -curr.bottom,
        ),
    };

    (cx < ax) || (cx > bx + x) || ((cy - ay).abs() > y)
}

/// Group characters into words.
pub(crate) fn iter_chars_to_words<'a>(
    ordered: &'a [&'a CharObj],
    direction: TextDir,
    settings: &TextSettings,
) -> Vec<Vec<&'a CharObj>> {
    let mut words: Vec<Vec<&CharObj>> = Vec::new();
    let mut current: Vec<&CharObj> = Vec::new();

    let xt = settings.x_tolerance;
    let yt = settings.y_tolerance;
    let xtr = settings.x_tolerance_ratio;
    let ytr = settings.y_tolerance_ratio;

    for &char in ordered {
        let text = &char.text;
        if !settings.keep_blank_chars && text.chars().all(|c| c.is_whitespace()) {
            if !current.is_empty() {
                words.push(current);
                current = Vec::new();
            }
        } else if settings.split_at_punctuation.contains(text) {
            if !current.is_empty() {
                words.push(current);
            }
            words.push(vec![char]);
            current = Vec::new();
        } else if !current.is_empty() {
            let prev = current.last().unwrap();
            let xtol = xtr.map(|r| r * prev.size).unwrap_or(xt);
            let ytol = ytr.map(|r| r * prev.size).unwrap_or(yt);
            if char_begins_new_word(prev, char, direction, xtol, ytol) {
                words.push(current);
                current = vec![char];
            } else {
                current.push(char);
            }
        } else {
            current.push(char);
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// Group characters into lines.
fn iter_chars_to_lines<'a>(
    chars: &'a [&'a CharObj],
    settings: &TextSettings,
) -> Vec<(Vec<&'a CharObj>, TextDir)> {
    let upright = chars.first().map(|c| c.upright).unwrap_or(true);
    let line_dir = if upright {
        settings.line_dir
    } else {
        settings.line_dir_rotated.unwrap_or(settings.char_dir)
    };
    let char_dir = get_char_dir(upright, settings);

    let line_cluster_key = |c: &&CharObj| match line_dir {
        TextDir::Ttb => c.top,
        TextDir::Btt => -c.bottom,
        TextDir::Ltr => c.x0,
        TextDir::Rtl => -c.x1,
    };

    let char_sort_key = |c: &&CharObj| get_char_sort_key(char_dir, c);

    let tolerance = if matches!(line_dir, TextDir::Ttb | TextDir::Btt) {
        settings.y_tolerance
    } else {
        settings.x_tolerance
    };

    let subclusters = cluster_objects(chars, line_cluster_key, tolerance, false);
    let mut out = Vec::new();
    for sc in subclusters {
        let mut sorted = sc;
        sorted.sort_by(|a, b| {
            let ka = char_sort_key(a);
            let kb = char_sort_key(b);
            ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
        });
        out.push((sorted, char_dir));
    }
    out
}

/// Extract words from characters.
pub(crate) fn extract_words(chars: &[CharObj], settings: &TextSettings) -> Vec<WordObj> {
    if chars.is_empty() {
        return Vec::new();
    }
    let refs: Vec<&CharObj> = chars.iter().collect();
    extract_words_refs(&refs, settings)
}

/// Extract words from character references.
fn extract_words_refs<'a>(chars: &'a [&'a CharObj], settings: &TextSettings) -> Vec<WordObj> {
    if chars.is_empty() {
        return Vec::new();
    }
    let mut grouped: HashMap<(bool, String), Vec<&CharObj>> = HashMap::new();
    for &c in chars {
        let key = (c.upright, String::new());
        grouped.entry(key).or_default().push(c);
    }

    let mut words = Vec::new();
    for (_key, group) in grouped {
        let line_groups = if settings.use_text_flow {
            vec![(group.clone(), settings.char_dir)]
        } else {
            iter_chars_to_lines(&group, settings)
        };
        for (line_chars, direction) in line_groups {
            for word_chars in iter_chars_to_words(&line_chars, direction, settings) {
                words.push(merge_chars(&word_chars, settings));
            }
        }
    }
    words
}

/// Convert lines to text string with proper direction handling.
fn textmap_to_string(lines: Vec<String>, line_dir: TextDir, char_dir: TextDir) -> String {
    let mut lines = lines;
    if matches!(line_dir, TextDir::Btt | TextDir::Rtl) {
        lines.reverse();
    }
    if char_dir == TextDir::Rtl {
        lines = lines
            .into_iter()
            .map(|l| l.chars().rev().collect::<String>())
            .collect();
    }
    if matches!(line_dir, TextDir::Rtl | TextDir::Ltr) {
        let max_len = lines.iter().map(|l| l.len()).max().unwrap_or(0);
        let mut new_lines = Vec::new();
        for line in lines {
            if char_dir == TextDir::Btt {
                new_lines.push(format!("{}{}", " ".repeat(max_len - line.len()), line));
            } else {
                new_lines.push(format!("{}{}", line, " ".repeat(max_len - line.len())));
            }
        }
        let mut out = String::new();
        for i in 0..max_len {
            for line in &new_lines {
                out.push(line.chars().nth(i).unwrap_or(' '));
            }
            if i + 1 < max_len {
                out.push('\n');
            }
        }
        return out;
    }
    lines.join("\n")
}

/// Extract text from characters.
pub(crate) fn extract_text(chars: &[CharObj], settings: &TextSettings) -> String {
    if chars.is_empty() {
        return String::new();
    }
    let refs: Vec<&CharObj> = chars.iter().collect();
    extract_text_refs(&refs, settings)
}

/// Extract text from character references.
fn extract_text_refs(chars: &[&CharObj], settings: &TextSettings) -> String {
    if chars.is_empty() {
        return String::new();
    }
    let words = extract_words_refs(chars, settings);

    let line_dir_render = settings.line_dir;
    let char_dir_render = settings.char_dir;

    let line_cluster_key = |w: &WordObj| get_line_cluster_key(settings.line_dir, w);
    let tolerance = if matches!(line_dir_render, TextDir::Ttb | TextDir::Btt) {
        settings.y_tolerance
    } else {
        settings.x_tolerance
    };

    let lines = cluster_objects(&words, line_cluster_key, tolerance, false);

    let mut line_texts = Vec::new();
    for line in lines {
        let mut line_sorted = line;
        line_sorted.sort_by(|a, b| {
            let key_a = match char_dir_render {
                TextDir::Ltr => a.x0,
                TextDir::Rtl => -a.x1,
                TextDir::Ttb => a.top,
                TextDir::Btt => -a.bottom,
            };
            let key_b = match char_dir_render {
                TextDir::Ltr => b.x0,
                TextDir::Rtl => -b.x1,
                TextDir::Ttb => b.top,
                TextDir::Btt => -b.bottom,
            };
            key_a
                .partial_cmp(&key_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let line_str = line_sorted
            .iter()
            .map(|w| w.text.clone())
            .collect::<Vec<_>>()
            .join(" ");
        line_texts.push(line_str);
    }

    textmap_to_string(line_texts, line_dir_render, char_dir_render)
}

/// Extract text from specific character indices.
pub(crate) fn extract_text_from_char_ids(
    chars: &[CharObj],
    ids: &[CharId],
    settings: &TextSettings,
) -> String {
    if ids.is_empty() {
        return String::new();
    }
    let mut refs: Vec<&CharObj> = Vec::with_capacity(ids.len());
    for id in ids {
        refs.push(&chars[id.0]);
    }
    extract_text_refs(&refs, settings)
}
