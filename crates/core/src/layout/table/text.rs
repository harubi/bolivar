//! Text extraction and word formation for table cells.
//!
//! This module handles converting characters to words and extracting
//! text from table cells with proper text direction handling.

use std::collections::HashMap;

use crate::arena::ArenaLookup;
use crate::layout::bidi::reorder_text_per_line;

use super::clustering::{bbox_from_chars, cluster_objects};
use super::types::{CharId, CharObj, TextDir, TextSettings, WordObj};

const DEFAULT_X_DENSITY: f64 = 7.25;
const DEFAULT_Y_DENSITY: f64 = 13.0;

fn char_text<'a>(obj: &CharObj, arena: &'a dyn ArenaLookup) -> &'a str {
    arena.resolve(obj.text)
}

fn maybe_reorder_bidi_default(text: String, settings: &TextSettings) -> String {
    if settings.horizontal_ltr
        && settings.line_dir == TextDir::Ttb
        && settings.char_dir == TextDir::Ltr
    {
        return reorder_text_per_line(&text);
    }
    text
}

/// Get the line cluster key for a word based on text direction.
pub fn get_line_cluster_key(dir: TextDir, obj: &WordObj) -> f64 {
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
pub fn merge_chars(
    ordered: &[&CharObj],
    settings: &TextSettings,
    arena: &dyn ArenaLookup,
) -> WordObj {
    let bbox = bbox_from_chars(ordered);
    let doctop_adj = ordered[0].doctop - ordered[0].top;
    let upright = ordered[0].upright;
    let char_dir = get_char_dir(upright, settings);

    let text = ordered
        .iter()
        .map(|c| expand_ligature(char_text(c, arena), settings.expand_ligatures))
        .collect::<String>();
    let text = maybe_reorder_bidi_default(text, settings);

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
pub fn iter_chars_to_words<'a>(
    ordered: &[&'a CharObj],
    direction: TextDir,
    settings: &TextSettings,
    arena: &dyn ArenaLookup,
) -> Vec<Vec<&'a CharObj>> {
    let mut words: Vec<Vec<&CharObj>> = Vec::new();
    let mut current: Vec<&CharObj> = Vec::new();

    let xt = settings.x_tolerance;
    let yt = settings.y_tolerance;
    let xtr = settings.x_tolerance_ratio;
    let ytr = settings.y_tolerance_ratio;

    for &char in ordered {
        let text = char_text(char, arena);
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
    chars: &[&'a CharObj],
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
pub fn extract_words(
    chars: &[CharObj],
    settings: &TextSettings,
    arena: &dyn ArenaLookup,
) -> Vec<WordObj> {
    if chars.is_empty() {
        return Vec::new();
    }
    let refs: Vec<&CharObj> = chars.iter().collect();
    extract_words_refs(&refs, settings, arena)
}

/// Extract words from character references.
fn extract_words_refs<'a>(
    chars: &'a [&'a CharObj],
    settings: &TextSettings,
    arena: &dyn ArenaLookup,
) -> Vec<WordObj> {
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
            for word_chars in iter_chars_to_words(&line_chars, direction, settings, arena) {
                words.push(merge_chars(&word_chars, settings, arena));
            }
        }
    }
    words
}

fn extract_word_map<'a>(
    chars: &'a [&'a CharObj],
    settings: &TextSettings,
    arena: &dyn ArenaLookup,
) -> Vec<(WordObj, Vec<&'a CharObj>)> {
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
            for word_chars in iter_chars_to_words(&line_chars, direction, settings, arena) {
                let word = merge_chars(&word_chars, settings, arena);
                words.push((word, word_chars));
            }
        }
    }
    words
}

const fn bbox_origin(bbox: &super::types::BBox, dir: TextDir) -> f64 {
    match dir {
        TextDir::Ttb => bbox.top,
        TextDir::Btt => bbox.bottom,
        TextDir::Ltr => bbox.x0,
        TextDir::Rtl => bbox.x1,
    }
}

const fn word_position(word: &WordObj, dir: TextDir) -> f64 {
    match dir {
        TextDir::Ttb => word.top,
        TextDir::Btt => word.bottom,
        TextDir::Ltr => word.x0,
        TextDir::Rtl => word.x1,
    }
}

fn extract_text_layout_refs(
    chars: &[&CharObj],
    settings: &TextSettings,
    layout_bbox: &super::types::BBox,
    arena: &dyn ArenaLookup,
) -> String {
    let word_map = extract_word_map(chars, settings, arena);
    if word_map.is_empty() {
        return String::new();
    }

    let layout_width = layout_bbox.x1 - layout_bbox.x0;
    let layout_height = layout_bbox.bottom - layout_bbox.top;
    let layout_width_chars = (layout_width / DEFAULT_X_DENSITY).round() as i64;
    let layout_height_chars = (layout_height / DEFAULT_Y_DENSITY).round() as i64;

    let line_dir = settings.line_dir;
    let char_dir = settings.char_dir;

    let line_cluster_key = |w: &(WordObj, Vec<&CharObj>)| get_line_cluster_key(line_dir, &w.0);
    let tolerance = if matches!(line_dir, TextDir::Ttb | TextDir::Btt) {
        settings.y_tolerance
    } else {
        settings.x_tolerance
    };

    let mut tuples = word_map;
    if !settings.use_text_flow {
        tuples.sort_by(|a, b| {
            line_cluster_key(a)
                .partial_cmp(&line_cluster_key(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    let tuples_by_line =
        cluster_objects(&tuples, line_cluster_key, tolerance, settings.use_text_flow);

    let y_origin = bbox_origin(layout_bbox, line_dir);
    let x_origin = bbox_origin(layout_bbox, char_dir);

    let mut out = String::new();
    let mut num_newlines: i64 = 0;

    for (line_idx, line_tuples) in tuples_by_line.into_iter().enumerate() {
        let y_dist = {
            let line_position = word_position(&line_tuples[0].0, line_dir);
            let adj = if matches!(line_dir, TextDir::Btt | TextDir::Rtl) {
                -1.0
            } else {
                1.0
            };
            (line_position - y_origin) * adj / DEFAULT_Y_DENSITY
        };

        let num_newlines_prepend = std::cmp::max(
            if line_idx > 0 { 1 } else { 0 },
            y_dist.round() as i64 - num_newlines,
        );

        for _ in 0..num_newlines_prepend {
            if (out.is_empty() || out.ends_with('\n')) && layout_width_chars > 0 {
                out.push_str(&" ".repeat(layout_width_chars as usize));
            }
            out.push('\n');
        }

        num_newlines += num_newlines_prepend;

        let mut line_len: i64 = 0;

        let mut line_sorted = line_tuples;
        if !settings.use_text_flow {
            line_sorted.sort_by(|a, b| {
                let key_a = match char_dir {
                    TextDir::Ltr => a.0.x0,
                    TextDir::Rtl => -a.0.x1,
                    TextDir::Ttb => a.0.top,
                    TextDir::Btt => -a.0.bottom,
                };
                let key_b = match char_dir {
                    TextDir::Ltr => b.0.x0,
                    TextDir::Rtl => -b.0.x1,
                    TextDir::Ttb => b.0.top,
                    TextDir::Btt => -b.0.bottom,
                };
                key_a
                    .partial_cmp(&key_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        for (word, chars) in line_sorted {
            let x_dist = {
                let char_position = word_position(&word, char_dir);
                let adj = if matches!(char_dir, TextDir::Btt | TextDir::Rtl) {
                    -1.0
                } else {
                    1.0
                };
                (char_position - x_origin) * adj / DEFAULT_X_DENSITY
            };

            let min_space = if line_len > 0 { 1 } else { 0 };
            let num_spaces_prepend = std::cmp::max(min_space, x_dist.round() as i64 - line_len);
            if num_spaces_prepend > 0 {
                out.push_str(&" ".repeat(num_spaces_prepend as usize));
                line_len += num_spaces_prepend;
            }

            for c in chars {
                let expanded = expand_ligature(char_text(c, arena), settings.expand_ligatures);
                for ch in expanded.chars() {
                    out.push(ch);
                    line_len += 1;
                }
            }
        }

        if layout_width_chars > 0 {
            let pad = layout_width_chars - line_len;
            if pad > 0 {
                out.push_str(&" ".repeat(pad as usize));
            }
        }
    }

    if layout_height_chars > 0 {
        let num_newlines_append = layout_height_chars - (num_newlines + 1);
        for i in 0..num_newlines_append {
            if i > 0 && layout_width_chars > 0 {
                out.push_str(&" ".repeat(layout_width_chars as usize));
            }
            out.push('\n');
        }
        if out.ends_with('\n') {
            out.pop();
        }
    }

    maybe_reorder_bidi_default(out, settings)
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
pub fn extract_text(chars: &[CharObj], settings: &TextSettings, arena: &dyn ArenaLookup) -> String {
    if chars.is_empty() {
        return String::new();
    }
    let refs: Vec<&CharObj> = chars.iter().collect();
    extract_text_refs(&refs, settings, arena)
}

/// Extract text from character references.
fn extract_text_refs(
    chars: &[&CharObj],
    settings: &TextSettings,
    arena: &dyn ArenaLookup,
) -> String {
    if chars.is_empty() {
        return String::new();
    }
    let words = extract_words_refs(chars, settings, arena);

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

    // `merge_chars` already applies default bidi reordering at word level.
    // Reordering the full line again here can invert pure-RTL words back.
    textmap_to_string(line_texts, line_dir_render, char_dir_render)
}

/// Extract text from specific character indices.
pub fn extract_text_from_char_ids(
    chars: &[CharObj],
    ids: &[CharId],
    settings: &TextSettings,
    arena: &dyn ArenaLookup,
) -> String {
    if ids.is_empty() {
        return String::new();
    }
    let mut refs: Vec<&CharObj> = Vec::with_capacity(ids.len());
    for id in ids {
        refs.push(&chars[id.0]);
    }
    extract_text_refs(&refs, settings, arena)
}

/// Extract text from specific character indices with layout spacing.
pub fn extract_text_from_char_ids_layout(
    chars: &[CharObj],
    ids: &[CharId],
    settings: &TextSettings,
    layout_bbox: &super::types::BBox,
    arena: &dyn ArenaLookup,
) -> String {
    if ids.is_empty() {
        return String::new();
    }
    let mut refs: Vec<&CharObj> = Vec::with_capacity(ids.len());
    for id in ids {
        refs.push(&chars[id.0]);
    }
    extract_text_layout_refs(&refs, settings, layout_bbox, arena)
}
