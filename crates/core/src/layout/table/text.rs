//! Text extraction and word formation for table cells.
//!
//! This module handles converting characters to words and extracting
//! text from table cells with proper text direction handling.

use std::collections::HashMap;

use crate::arena::ArenaLookup;
use crate::cancellation::CancellationToken;
use crate::error::Result;
use crate::layout::bidi::{
    contains_rtl_text, has_compact_mixed_token, is_ltr_prefixed_compact_mixed,
    reconstruct_text_for_output, reconstruct_words, reorder_text_for_output,
    reorder_visual_word_runs,
};

use super::clustering::cluster_objects;
use super::types::{CharId, CharObj, TextDir, TextSettings, WordObj};

/// One span of a cell's text, and the word on the page it came from.
///
/// Bidi reconstruction reorders words, so the order a reader sees is not the
/// order the page laid down. `start` and `end` are character offsets into the
/// extracted text; `word_index` is the word's position in the line's geometric
/// order. Two spans sharing a `word_index` were one word before reordering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextSpan {
    pub text: String,
    pub line_index: usize,
    pub word_index: usize,
    pub start: usize,
    pub end: usize,
}

/// Whether a line's spans could be established at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanFidelity {
    /// Every span is anchored to the word it came from.
    Exact,
    /// The line was reordered by a path that carries no word identity, so no
    /// span is claimed for it rather than guessing one.
    Unavailable,
}

const DEFAULT_X_DENSITY: f64 = 7.25;
const DEFAULT_Y_DENSITY: f64 = 13.0;
const CANCEL_INTERVAL: usize = 256;

fn char_text<'a>(obj: &CharObj, arena: &'a dyn ArenaLookup) -> &'a str {
    arena.resolve(obj.text)
}

fn maybe_reorder_bidi_default(text: String, settings: &TextSettings) -> String {
    if settings.horizontal_ltr
        && settings.line_dir == TextDir::Ttb
        && settings.char_dir == TextDir::Ltr
    {
        return if settings.bidi && has_compact_mixed_token(&text) {
            reconstruct_text_for_output(&text)
        } else {
            reorder_text_for_output(&text)
        };
    }
    text
}

fn should_reconstruct_geometric_line(words: &[WordObj]) -> bool {
    let Some((last, prefix)) = words.split_last() else {
        return false;
    };
    // Geometry must show an LTR prefix followed by one compact mixed tail.
    // Other layouts keep the stable table word order.
    is_ltr_prefixed_compact_mixed(&last.text)
        && prefix.iter().all(|word| !contains_rtl_text(&word.text))
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

fn merge_chars_with_bidi(
    ordered: &[&CharObj],
    settings: &TextSettings,
    arena: &dyn ArenaLookup,
    reconstruct_bidi: bool,
) -> WordObj {
    merge_chars_cancellable(
        ordered,
        settings,
        arena,
        reconstruct_bidi,
        &CancellationToken::new(),
    )
    .expect("a new cancellation token cannot be cancelled")
}

fn merge_chars_cancellable(
    ordered: &[&CharObj],
    settings: &TextSettings,
    arena: &dyn ArenaLookup,
    reconstruct_bidi: bool,
    cancellation: &CancellationToken,
) -> Result<WordObj> {
    cancellation.check()?;
    let mut x0 = f64::INFINITY;
    let mut top = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut bottom = f64::NEG_INFINITY;
    let mut text = String::new();
    for (index, char) in ordered.iter().enumerate() {
        if index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        x0 = x0.min(char.x0);
        top = top.min(char.top);
        x1 = x1.max(char.x1);
        bottom = bottom.max(char.bottom);
        text.push_str(expand_ligature(
            char_text(char, arena),
            settings.expand_ligatures,
        ));
    }
    cancellation.check()?;

    let bbox = super::types::BBox {
        x0,
        top,
        x1,
        bottom,
    };
    let doctop_adj = ordered[0].doctop - ordered[0].top;
    let upright = ordered[0].upright;
    let char_dir = get_char_dir(upright, settings);

    let text = if reconstruct_bidi {
        maybe_reorder_bidi_default(text, settings)
    } else {
        text
    };

    Ok(WordObj {
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
    })
}

/// Expand ligature characters to their component characters.
fn expand_ligature(text: &str, expand: bool) -> &str {
    if !expand {
        return text;
    }
    match text {
        "\u{fb00}" => "ff",
        "\u{fb03}" => "ffi",
        "\u{fb04}" => "ffl",
        "\u{fb01}" => "fi",
        "\u{fb02}" => "fl",
        "\u{fb06}" => "st",
        "\u{fb05}" => "st",
        _ => text,
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
    iter_chars_to_lines_cancellable(chars, settings, &CancellationToken::new())
        .expect("a new cancellation token cannot be cancelled")
}

fn iter_chars_to_lines_cancellable<'a>(
    chars: &[&'a CharObj],
    settings: &TextSettings,
    cancellation: &CancellationToken,
) -> Result<Vec<(Vec<&'a CharObj>, TextDir)>> {
    cancellation.check()?;
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

    let mut sorted = chars.to_vec();
    sorted.sort_by(|first, second| {
        line_cluster_key(first)
            .partial_cmp(&line_cluster_key(second))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    cancellation.check()?;

    let mut subclusters = Vec::new();
    let mut current = Vec::new();
    let mut last_key = None;
    for (index, char) in sorted.into_iter().enumerate() {
        if index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        let key = line_cluster_key(&char);
        if last_key.is_some_and(|last| key > last + tolerance) {
            subclusters.push(current);
            current = Vec::new();
        }
        current.push(char);
        last_key = Some(key);
    }
    if !current.is_empty() {
        subclusters.push(current);
    }

    let mut out = Vec::new();
    for (index, sc) in subclusters.into_iter().enumerate() {
        if index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        let mut sorted = sc;
        sorted.sort_by(|a, b| {
            let ka = char_sort_key(a);
            let kb = char_sort_key(b);
            ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
        });
        cancellation.check()?;
        out.push((sorted, char_dir));
    }
    Ok(out)
}

fn push_word(
    chars: &[&CharObj],
    settings: &TextSettings,
    arena: &dyn ArenaLookup,
    words: &mut Vec<WordObj>,
    cancellation: &CancellationToken,
) -> Result<()> {
    words.push(merge_chars_cancellable(
        chars,
        settings,
        arena,
        true,
        cancellation,
    )?);
    Ok(())
}

fn append_line_words(
    line_chars: &[&CharObj],
    direction: TextDir,
    settings: &TextSettings,
    arena: &dyn ArenaLookup,
    words: &mut Vec<WordObj>,
    cancellation: &CancellationToken,
) -> Result<()> {
    cancellation.check()?;
    let mut word_start = None;
    for (index, &char) in line_chars.iter().enumerate() {
        if index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        let text = char_text(char, arena);
        if !settings.keep_blank_chars && text.chars().all(char::is_whitespace) {
            if let Some(start) = word_start.take() {
                push_word(
                    &line_chars[start..index],
                    settings,
                    arena,
                    words,
                    cancellation,
                )?;
            }
            continue;
        }

        if settings.split_at_punctuation.contains(text) {
            if let Some(start) = word_start.take() {
                push_word(
                    &line_chars[start..index],
                    settings,
                    arena,
                    words,
                    cancellation,
                )?;
            }
            push_word(
                &line_chars[index..=index],
                settings,
                arena,
                words,
                cancellation,
            )?;
            continue;
        }

        let Some(start) = word_start else {
            word_start = Some(index);
            continue;
        };
        let previous = line_chars[index - 1];
        let x_tolerance = settings
            .x_tolerance_ratio
            .map(|ratio| ratio * previous.size)
            .unwrap_or(settings.x_tolerance);
        let y_tolerance = settings
            .y_tolerance_ratio
            .map(|ratio| ratio * previous.size)
            .unwrap_or(settings.y_tolerance);
        if char_begins_new_word(previous, char, direction, x_tolerance, y_tolerance) {
            push_word(
                &line_chars[start..index],
                settings,
                arena,
                words,
                cancellation,
            )?;
            word_start = Some(index);
        }
    }
    if let Some(start) = word_start {
        push_word(&line_chars[start..], settings, arena, words, cancellation)?;
    }
    Ok(())
}

/// Extract words from characters.
pub fn extract_words(
    chars: &[CharObj],
    settings: &TextSettings,
    arena: &dyn ArenaLookup,
) -> Vec<WordObj> {
    extract_words_cancellable(chars, settings, arena, &CancellationToken::new())
        .expect("a new cancellation token cannot be cancelled")
}

pub(super) fn extract_words_cancellable(
    chars: &[CharObj],
    settings: &TextSettings,
    arena: &dyn ArenaLookup,
    cancellation: &CancellationToken,
) -> Result<Vec<WordObj>> {
    cancellation.check()?;
    if chars.is_empty() {
        return Ok(Vec::new());
    }
    let mut refs = Vec::with_capacity(chars.len());
    for (index, char) in chars.iter().enumerate() {
        if index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        refs.push(char);
    }
    extract_words_refs_cancellable(&refs, settings, arena, cancellation)
}

/// Extract words from character references.
fn extract_words_refs<'a>(
    chars: &'a [&'a CharObj],
    settings: &TextSettings,
    arena: &dyn ArenaLookup,
) -> Vec<WordObj> {
    extract_words_refs_cancellable(chars, settings, arena, &CancellationToken::new())
        .expect("a new cancellation token cannot be cancelled")
}

fn extract_words_refs_cancellable<'a>(
    chars: &'a [&'a CharObj],
    settings: &TextSettings,
    arena: &dyn ArenaLookup,
    cancellation: &CancellationToken,
) -> Result<Vec<WordObj>> {
    cancellation.check()?;
    if chars.is_empty() {
        return Ok(Vec::new());
    }

    let upright = chars[0].upright;
    let mut same_orientation = true;
    for (index, char) in chars.iter().enumerate() {
        if index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        if char.upright != upright {
            same_orientation = false;
            break;
        }
    }

    if same_orientation {
        let line_groups = if settings.use_text_flow {
            vec![(chars.to_vec(), settings.char_dir)]
        } else {
            iter_chars_to_lines_cancellable(chars, settings, cancellation)?
        };
        let mut words = Vec::new();
        for (line_chars, direction) in line_groups {
            append_line_words(
                &line_chars,
                direction,
                settings,
                arena,
                &mut words,
                cancellation,
            )?;
        }
        return Ok(words);
    }

    let mut grouped: HashMap<(bool, String), Vec<&CharObj>> = HashMap::new();
    for (index, &char) in chars.iter().enumerate() {
        if index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        let key = (char.upright, String::new());
        grouped.entry(key).or_default().push(char);
    }

    let mut words = Vec::new();
    for (index, (_key, group)) in grouped.into_iter().enumerate() {
        if index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        let line_groups = if settings.use_text_flow {
            vec![(group.clone(), settings.char_dir)]
        } else {
            iter_chars_to_lines_cancellable(&group, settings, cancellation)?
        };
        for (line_chars, direction) in line_groups {
            append_line_words(
                &line_chars,
                direction,
                settings,
                arena,
                &mut words,
                cancellation,
            )?;
        }
    }
    Ok(words)
}

fn extract_word_map<'a>(
    chars: &'a [&'a CharObj],
    settings: &TextSettings,
    arena: &dyn ArenaLookup,
    reconstruct_bidi: bool,
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
                let word = merge_chars_with_bidi(&word_chars, settings, arena, reconstruct_bidi);
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
    let word_map = extract_word_map(chars, settings, arena, !settings.bidi);
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
    extract_text_refs_inner(chars, settings, arena, false).0
}

/// Extract text and, where the layout permits it, the source word of each run.
///
/// Runs are only offered when the lines are stacked top to bottom and read left
/// to right, because `textmap_to_string` transposes or reverses every other
/// combination and a character offset into the result would not address the run
/// it came from. In those cases the text is returned and the run list is empty:
/// no offset is invented.
fn extract_text_refs_inner(
    chars: &[&CharObj],
    settings: &TextSettings,
    arena: &dyn ArenaLookup,
    want_spans: bool,
) -> (String, Vec<TextSpan>) {
    if chars.is_empty() {
        return (String::new(), Vec::new());
    }
    let words = if settings.bidi {
        extract_word_map(chars, settings, arena, false)
            .into_iter()
            .map(|(word, _)| word)
            .collect::<Vec<_>>()
    } else {
        extract_words_refs(chars, settings, arena)
    };

    let line_dir_render = settings.line_dir;
    let char_dir_render = settings.char_dir;

    let line_cluster_key = |w: &WordObj| get_line_cluster_key(settings.line_dir, w);
    let tolerance = if matches!(line_dir_render, TextDir::Ttb | TextDir::Btt) {
        settings.y_tolerance
    } else {
        settings.x_tolerance
    };

    let mut words = words;
    words.sort_by(|first, second| {
        line_cluster_key(first)
            .partial_cmp(&line_cluster_key(second))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut lines = Vec::new();
    let mut current = Vec::new();
    let mut last_key = None;
    for word in words {
        let key = line_cluster_key(&word);
        if last_key.is_some_and(|last| key > last + tolerance) {
            lines.push(current);
            current = Vec::new();
        }
        current.push(word);
        last_key = Some(key);
    }
    if !current.is_empty() {
        lines.push(current);
    }

    // Offsets only address the result when lines are joined with a newline in
    // the order they were built. Anything else is transposed or reversed.
    let offsets_addressable = line_dir_render == TextDir::Ttb && char_dir_render != TextDir::Rtl;
    let collect_spans = want_spans && offsets_addressable;

    let mut line_texts = Vec::new();
    let mut per_line: Vec<Vec<(String, usize)>> = Vec::new();

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
        let mut pieces: Vec<(String, usize)> = Vec::new();
        if settings.bidi {
            if should_reconstruct_geometric_line(&line_sorted) {
                // Same text as the string path, but keeps the source word of
                // each output run.
                let words: Vec<&str> = line_sorted.iter().map(|word| word.text.as_str()).collect();
                let reconstructed = reconstruct_words(&words, " ");
                if collect_spans {
                    for span in &reconstructed.spans {
                        pieces.push((span.text.clone(), span.source_index));
                    }
                }
                line_texts.push(reconstructed.text);
            } else {
                let indexed: Vec<(usize, &WordObj)> = line_sorted.iter().enumerate().collect();
                let legacy_order =
                    reorder_visual_word_runs(indexed, |(_, word)| word.text.as_str());
                let mut text = String::new();
                for (position, (word_index, word)) in legacy_order.into_iter().enumerate() {
                    if position > 0 {
                        text.push(' ');
                        if collect_spans {
                            pieces.push((" ".to_string(), word_index));
                        }
                    }
                    let piece = reorder_text_for_output(&word.text);
                    if collect_spans {
                        pieces.push((piece.clone(), word_index));
                    }
                    text.push_str(&piece);
                }
                line_texts.push(text);
            }
        } else {
            if !collect_spans {
                let ordered = if char_dir_render == TextDir::Ltr {
                    reorder_visual_word_runs(line_sorted, |word| word.text.as_str())
                } else {
                    line_sorted
                };
                let mut text = String::new();
                for (position, word) in ordered.into_iter().enumerate() {
                    if position > 0 {
                        text.push(' ');
                    }
                    text.push_str(&word.text);
                }
                line_texts.push(text);
                continue;
            }

            let mut indexed: Vec<(usize, &WordObj)> = line_sorted.iter().enumerate().collect();
            if char_dir_render == TextDir::Ltr {
                indexed = reorder_visual_word_runs(indexed, |(_, word)| word.text.as_str());
            }
            let mut text = String::new();
            for (position, (word_index, word)) in indexed.into_iter().enumerate() {
                if position > 0 {
                    text.push(' ');
                    if collect_spans {
                        pieces.push((" ".to_string(), word_index));
                    }
                }
                if collect_spans {
                    pieces.push((word.text.clone(), word_index));
                }
                text.push_str(&word.text);
            }
            line_texts.push(text);
        }
        if collect_spans {
            per_line.push(pieces);
        }
    }

    let text = textmap_to_string(line_texts, line_dir_render, char_dir_render);
    if !collect_spans {
        return (text, Vec::new());
    }

    // Lines were joined with a single newline, so a line's start is every
    // earlier line's characters plus one separator each.
    let mut out = Vec::new();
    let mut cursor = 0usize;
    for (line_index, pieces) in per_line.into_iter().enumerate() {
        if line_index > 0 {
            cursor += 1;
        }
        for (piece, word_index) in pieces {
            let length = piece.chars().count();
            if piece.trim().is_empty() {
                cursor += length;
                continue;
            }
            out.push(TextSpan {
                text: piece,
                line_index,
                word_index,
                start: cursor,
                end: cursor + length,
            });
            cursor += length;
        }
    }
    (text, out)
}

fn char_refs<I>(chars: &[CharObj], ids: I) -> Vec<&CharObj>
where
    I: ExactSizeIterator<Item = CharId>,
{
    let mut refs: Vec<&CharObj> = Vec::with_capacity(ids.len());
    for id in ids {
        refs.push(&chars[id.index()]);
    }
    refs
}

/// Extract text from specific character indices.
pub(super) fn extract_text_from_id_iter<I>(
    chars: &[CharObj],
    ids: I,
    settings: &TextSettings,
    arena: &dyn ArenaLookup,
) -> String
where
    I: ExactSizeIterator<Item = CharId>,
{
    let refs = char_refs(chars, ids);
    extract_text_refs(&refs, settings, arena)
}

/// Extract text from specific character indices with layout spacing.
pub(super) fn extract_layout_from_id_iter<I>(
    chars: &[CharObj],
    ids: I,
    settings: &TextSettings,
    layout_bbox: &super::types::BBox,
    arena: &dyn ArenaLookup,
) -> String
where
    I: ExactSizeIterator<Item = CharId>,
{
    let refs = char_refs(chars, ids);
    if refs.is_empty() {
        return String::new();
    }
    extract_text_layout_refs(&refs, settings, layout_bbox, arena)
}

#[cfg(test)]
pub fn extract_text_from_char_ids_layout(
    chars: &[CharObj],
    ids: &[CharId],
    settings: &TextSettings,
    layout_bbox: &super::types::BBox,
    arena: &dyn ArenaLookup,
) -> String {
    extract_layout_from_id_iter(chars, ids.iter().copied(), settings, layout_bbox, arena)
}

/// Extract a cell's text together with the source word of each output run.
pub(super) fn extract_spans_from_id_iter<I>(
    chars: &[CharObj],
    ids: I,
    settings: &TextSettings,
    arena: &dyn ArenaLookup,
) -> (String, Vec<TextSpan>)
where
    I: ExactSizeIterator<Item = CharId>,
{
    let refs = char_refs(chars, ids);
    if refs.is_empty() {
        return (String::new(), Vec::new());
    }
    extract_text_refs_inner(&refs, settings, arena, true)
}

#[cfg(test)]
mod tests {
    use super::expand_ligature;

    #[test]
    fn normal_text_does_not_allocate_for_ligatures() {
        let text = String::from("plain");
        let expanded = expand_ligature(&text, true);

        assert_eq!(expanded.as_ptr(), text.as_ptr());
        assert_eq!(expand_ligature("\u{fb03}", true), "ffi");
    }
}
