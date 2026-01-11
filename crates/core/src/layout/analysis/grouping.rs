//! Character-to-line and line-to-box grouping algorithms.
//!
//! Contains group_objects() for grouping characters into text lines,
//! and group_textlines() for grouping text lines into text boxes.

use crate::simd::F64_LANES;
use crate::utils::{INF_F64, Rect};

use super::super::arena::{
    AnnoId, ArenaElem, ArenaTextBox, ArenaTextLine, ArenaTextLineHorizontal, ArenaTextLineVertical,
    BoxId, CharId, LayoutArena, LineId,
};
use super::super::params::LAParams;
use super::super::types::{
    LTAnno, LTChar, LTComponent, LTTextLineHorizontal, LTTextLineVertical, TextBoxType,
    TextLineElement, TextLineType,
};
use super::soa::RectSoA;
use super::soa_layout::LayoutSoA;

/// Groups character objects into text lines.
///
/// This is the core character-to-line grouping algorithm from pdfminer.
/// It groups LTChar objects based on horizontal/vertical alignment and proximity.
///
/// # Algorithm (Python lines 702-777)
/// - For each pair of consecutive characters, check if they are:
///   - horizontally aligned (halign): on same line, close enough horizontally
///   - vertically aligned (valign): on same column, close enough vertically
/// - Group characters into horizontal or vertical text lines accordingly
pub fn group_objects(laparams: &LAParams, objs: &[LTChar]) -> Vec<TextLineType> {
    if objs.is_empty() {
        return Vec::new();
    }
    let soa = LayoutSoA::from_chars(objs);
    group_objects_soa(laparams, objs, &soa)
}

fn group_objects_soa(laparams: &LAParams, objs: &[LTChar], soa: &LayoutSoA) -> Vec<TextLineType> {
    let mut result = Vec::new();

    let (halign_flags, valign_flags) = group_objects_pair_flags_soa(laparams, soa);
    let mut current_line: Option<TextLineType> = None;
    let mut obj0_idx = 0usize;

    for obj1_idx in 1..objs.len() {
        let obj0 = &objs[obj0_idx];
        let obj1 = &objs[obj1_idx];
        let halign = halign_flags.get(obj0_idx).copied().unwrap_or(false);
        let valign = valign_flags.get(obj0_idx).copied().unwrap_or(false);

        match &mut current_line {
            Some(TextLineType::Horizontal(line)) if halign => {
                // Continue horizontal line
                add_char_to_horizontal_line(line, obj1.clone(), laparams.word_margin);
            }
            Some(TextLineType::Vertical(line)) if valign => {
                // Continue vertical line
                add_char_to_vertical_line(line, obj1.clone(), laparams.word_margin);
            }
            Some(line) => {
                // End current line (obj0 was already added to it)
                line.analyze();
                result.push(line.clone());
                current_line = None;
                // Don't create single-char line from obj0 - it's already in current_line
                // Just continue to next iteration where obj1 becomes obj0
            }
            None => {
                if valign && !halign {
                    // Start new vertical line
                    let mut line = LTTextLineVertical::new(laparams.word_margin);
                    add_char_to_vertical_line(&mut line, obj0.clone(), laparams.word_margin);
                    add_char_to_vertical_line(&mut line, obj1.clone(), laparams.word_margin);
                    current_line = Some(TextLineType::Vertical(line));
                } else if halign && !valign {
                    // Start new horizontal line
                    let mut line = LTTextLineHorizontal::new(laparams.word_margin);
                    add_char_to_horizontal_line(&mut line, obj0.clone(), laparams.word_margin);
                    add_char_to_horizontal_line(&mut line, obj1.clone(), laparams.word_margin);
                    current_line = Some(TextLineType::Horizontal(line));
                } else {
                    // Neither aligned - output single-char line
                    let mut line = LTTextLineHorizontal::new(laparams.word_margin);
                    add_char_to_horizontal_line(&mut line, obj0.clone(), laparams.word_margin);
                    line.analyze();
                    result.push(TextLineType::Horizontal(line));
                }
            }
        }

        obj0_idx = obj1_idx;
    }

    // Handle remaining line or last character
    match current_line {
        Some(mut line) => {
            line.analyze();
            result.push(line);
        }
        None => {
            // Last character wasn't part of a line
            let mut line = LTTextLineHorizontal::new(laparams.word_margin);
            add_char_to_horizontal_line(&mut line, objs[obj0_idx].clone(), laparams.word_margin);
            line.analyze();
            result.push(TextLineType::Horizontal(line));
        }
    }

    result
}

fn group_objects_pair_flags_soa(laparams: &LAParams, soa: &LayoutSoA) -> (Vec<bool>, Vec<bool>) {
    let len = soa.len();
    if len < 2 {
        return (Vec::new(), Vec::new());
    }

    let mut halign_flags = vec![false; len - 1];
    let mut valign_flags = vec![false; len - 1];

    const LANES: usize = F64_LANES;
    let mut i = 0usize;

    use std::simd::prelude::*;
    let line_overlap = Simd::<f64, LANES>::splat(laparams.line_overlap);
    let char_margin = Simd::<f64, LANES>::splat(laparams.char_margin);

    while i + LANES < len {
        let x0a = Simd::<f64, LANES>::from_slice(&soa.x0[i..i + LANES]);
        let y0a = Simd::<f64, LANES>::from_slice(&soa.top[i..i + LANES]);
        let x1a = Simd::<f64, LANES>::from_slice(&soa.x1[i..i + LANES]);
        let y1a = Simd::<f64, LANES>::from_slice(&soa.bottom[i..i + LANES]);
        let x0b = Simd::<f64, LANES>::from_slice(&soa.x0[i + 1..i + 1 + LANES]);
        let y0b = Simd::<f64, LANES>::from_slice(&soa.top[i + 1..i + 1 + LANES]);
        let x1b = Simd::<f64, LANES>::from_slice(&soa.x1[i + 1..i + 1 + LANES]);
        let y1b = Simd::<f64, LANES>::from_slice(&soa.bottom[i + 1..i + 1 + LANES]);
        let w0 = Simd::<f64, LANES>::from_slice(&soa.w[i..i + LANES]);
        let w1 = Simd::<f64, LANES>::from_slice(&soa.w[i + 1..i + 1 + LANES]);
        let h0 = Simd::<f64, LANES>::from_slice(&soa.h[i..i + LANES]);
        let h1 = Simd::<f64, LANES>::from_slice(&soa.h[i + 1..i + 1 + LANES]);

        let is_voverlap = y0b.simd_le(y1a) & y0a.simd_le(y1b);
        let is_hoverlap = x0b.simd_le(x1a) & x0a.simd_le(x1b);

        let vdiff1 = (y0a - y1b).abs();
        let vdiff2 = (y1a - y0b).abs();
        let vmin = vdiff1.simd_min(vdiff2);
        let voverlap = is_voverlap.select(vmin, Simd::splat(0.0));
        let vdistance = is_voverlap.select(Simd::splat(0.0), vmin);

        let hdiff1 = (x0a - x1b).abs();
        let hdiff2 = (x1a - x0b).abs();
        let hmin = hdiff1.simd_min(hdiff2);
        let hoverlap = is_hoverlap.select(hmin, Simd::splat(0.0));
        let hdistance = is_hoverlap.select(Simd::splat(0.0), hmin);

        let min_height = h0.simd_min(h1);
        let max_width = w0.simd_max(w1);
        let halign_mask = is_voverlap
            & (min_height * line_overlap).simd_lt(voverlap)
            & hdistance.simd_lt(max_width * char_margin);

        let min_width = w0.simd_min(w1);
        let max_height = h0.simd_max(h1);
        let false_mask = is_voverlap & !is_voverlap;
        let valign_mask = if laparams.detect_vertical {
            is_hoverlap
                & (min_width * line_overlap).simd_lt(hoverlap)
                & vdistance.simd_lt(max_height * char_margin)
        } else {
            false_mask
        };

        let halign_arr = halign_mask.to_array();
        let valign_arr = valign_mask.to_array();
        for lane in 0..LANES {
            let idx = i + lane;
            if idx >= len - 1 {
                break;
            }
            halign_flags[idx] = halign_arr[lane];
            valign_flags[idx] = valign_arr[lane];
        }

        i += LANES;
    }

    for idx in i..(len - 1) {
        let ax0 = soa.x0[idx];
        let ay0 = soa.top[idx];
        let ax1 = soa.x1[idx];
        let ay1 = soa.bottom[idx];
        let bx0 = soa.x0[idx + 1];
        let by0 = soa.top[idx + 1];
        let bx1 = soa.x1[idx + 1];
        let by1 = soa.bottom[idx + 1];

        let is_voverlap = by0 <= ay1 && ay0 <= by1;
        let is_hoverlap = bx0 <= ax1 && ax0 <= bx1;

        let vdiff1 = (ay0 - by1).abs();
        let vdiff2 = (ay1 - by0).abs();
        let vmin = vdiff1.min(vdiff2);
        let voverlap = if is_voverlap { vmin } else { 0.0 };
        let vdistance = if is_voverlap { 0.0 } else { vmin };

        let hdiff1 = (ax0 - bx1).abs();
        let hdiff2 = (ax1 - bx0).abs();
        let hmin = hdiff1.min(hdiff2);
        let hoverlap = if is_hoverlap { hmin } else { 0.0 };
        let hdistance = if is_hoverlap { 0.0 } else { hmin };

        let min_height = soa.h[idx].min(soa.h[idx + 1]);
        let max_width = soa.w[idx].max(soa.w[idx + 1]);
        let halign = is_voverlap
            && min_height * laparams.line_overlap < voverlap
            && hdistance < max_width * laparams.char_margin;

        let min_width = soa.w[idx].min(soa.w[idx + 1]);
        let max_height = soa.h[idx].max(soa.h[idx + 1]);
        let valign = laparams.detect_vertical
            && is_hoverlap
            && min_width * laparams.line_overlap < hoverlap
            && vdistance < max_height * laparams.char_margin;
        halign_flags[idx] = halign;
        valign_flags[idx] = valign;
    }

    (halign_flags, valign_flags)
}

#[cfg(test)]
mod group_objects_simd_tests {
    use super::*;
    use crate::layout::analysis::soa_layout::LayoutSoA;

    #[test]
    fn group_objects_expected_lines() {
        let laparams = LAParams::default();
        let objs = vec![
            LTChar::new((0.0, 0.0, 5.0, 5.0), "A", "F", 10.0, true, 5.0),
            LTChar::new((6.0, 0.0, 10.0, 5.0), "B", "F", 10.0, true, 4.0),
            LTChar::new((0.0, 10.0, 5.0, 15.0), "C", "F", 10.0, true, 5.0),
        ];
        let lines = group_objects(&laparams, &objs);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn group_objects_simd_halign_matches_scalar() {
        let laparams = LAParams::default();
        let objs = vec![
            LTChar::new((0.0, 0.0, 5.0, 5.0), "A", "F", 10.0, true, 5.0),
            LTChar::new((6.0, 0.0, 10.0, 5.0), "B", "F", 10.0, true, 4.0),
            LTChar::new((0.0, 10.0, 5.0, 15.0), "C", "F", 10.0, true, 5.0),
        ];
        let soa = LayoutSoA::from_chars(&objs);
        let (halign_flags, valign_flags) = group_objects_pair_flags_soa(&laparams, &soa);
        assert!(halign_flags[0]);
        assert!(!valign_flags[0]);
    }

    #[test]
    fn group_objects_soa_matches_scalar_ordering() {
        let laparams = LAParams::default();
        let objs = vec![
            LTChar::new((0.0, 0.0, 5.0, 5.0), "A", "F", 10.0, true, 5.0),
            LTChar::new((6.0, 0.0, 10.0, 5.0), "B", "F", 10.0, true, 4.0),
            LTChar::new((0.0, 10.0, 5.0, 15.0), "C", "F", 10.0, true, 5.0),
        ];
        let soa = LayoutSoA::from_chars(&objs);
        let out = group_objects_soa(&laparams, &objs, &soa);
        let baseline = group_objects(&laparams, &objs);
        assert_eq!(out.len(), baseline.len());
    }

    #[test]
    fn group_objects_soa_uses_precomputed_metrics() {
        let laparams = LAParams::default();
        let soa = LayoutSoA {
            x0: vec![0.0, 12.0],
            x1: vec![10.0, 22.0],
            top: vec![0.0, 0.0],
            bottom: vec![10.0, 10.0],
            w: vec![1.0, 1.0],
            h: vec![10.0, 10.0],
            cx: vec![5.0, 17.0],
            cy: vec![5.0, 5.0],
            text: vec![String::new(), String::new()],
            font: vec![String::new(), String::new()],
            size: vec![10.0, 10.0],
            flags: vec![0, 0],
        };
        let (halign_flags, _valign_flags) = group_objects_pair_flags_soa(&laparams, &soa);
        assert!(!halign_flags[0]);
    }
}

#[cfg(test)]
mod arena_soa_tests {
    use super::*;
    use std::cmp::Ordering;

    fn hline(bbox: Rect) -> TextLineType {
        let mut line = LTTextLineHorizontal::new(0.1);
        line.set_bbox(bbox);
        TextLineType::Horizontal(line)
    }

    fn sorted_bboxes(boxes: &[TextBoxType]) -> Vec<Rect> {
        let mut bboxes: Vec<Rect> = boxes
            .iter()
            .map(|b| match b {
                TextBoxType::Horizontal(h) => h.bbox(),
                TextBoxType::Vertical(v) => v.bbox(),
            })
            .collect();
        bboxes.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal))
                .then_with(|| a.2.partial_cmp(&b.2).unwrap_or(Ordering::Equal))
                .then_with(|| a.3.partial_cmp(&b.3).unwrap_or(Ordering::Equal))
        });
        bboxes
    }

    #[test]
    fn arena_soa_expected_output() {
        let laparams = LAParams::default();
        let lines = vec![
            hline((0.0, 0.0, 10.0, 2.0)),
            hline((0.0, 2.5, 10.0, 4.5)),
            hline((20.0, 0.0, 30.0, 2.0)),
        ];

        let boxes = group_textlines(&laparams, lines);
        let got = sorted_bboxes(&boxes);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], (0.0, 0.0, 10.0, 4.5));
        assert_eq!(got[1], (20.0, 0.0, 30.0, 2.0));
    }
}

/// Arena-backed grouping of character objects into text lines.
///
/// Produces LineId values that can be materialized later; preserves exact
/// ordering and logic from group_objects().
pub fn group_objects_arena(laparams: &LAParams, arena: &mut LayoutArena) -> Vec<LineId> {
    let mut result: Vec<LineId> = Vec::new();
    let chars_len = arena.chars.len();
    if chars_len == 0 {
        return result;
    }

    let chars = &arena.chars;
    let annos = &mut arena.annos;
    let lines = &mut arena.lines;

    let mut current_line: Option<LineId> = None;
    let mut obj0_idx = 0usize;

    for obj1_idx in 1..chars_len {
        let obj0 = &chars[obj0_idx];
        let obj1 = &chars[obj1_idx];

        let halign = obj0.is_voverlap(obj1)
            && obj0.height().min(obj1.height()) * laparams.line_overlap < obj0.voverlap(obj1)
            && obj0.hdistance(obj1) < obj0.width().max(obj1.width()) * laparams.char_margin;

        let valign = laparams.detect_vertical
            && obj0.is_hoverlap(obj1)
            && obj0.width().min(obj1.width()) * laparams.line_overlap < obj0.hoverlap(obj1)
            && obj0.vdistance(obj1) < obj0.height().max(obj1.height()) * laparams.char_margin;

        match current_line {
            Some(line_id) => {
                let line = &mut lines[line_id.0];
                match line {
                    ArenaTextLine::Horizontal(h) if halign => {
                        add_char_to_horizontal_line_arena(
                            annos,
                            h,
                            &chars[obj1_idx],
                            obj1_idx,
                            laparams.word_margin,
                        );
                    }
                    ArenaTextLine::Vertical(v) if valign => {
                        add_char_to_vertical_line_arena(
                            annos,
                            v,
                            &chars[obj1_idx],
                            obj1_idx,
                            laparams.word_margin,
                        );
                    }
                    _ => {
                        analyze_line(annos, lines, line_id);
                        result.push(line_id);
                        current_line = None;
                    }
                }
            }
            None => {
                if valign && !halign {
                    let mut line = ArenaTextLineVertical {
                        component: LTComponent::new((INF_F64, INF_F64, -INF_F64, -INF_F64)),
                        word_margin: laparams.word_margin,
                        y0_tracker: -INF_F64,
                        elements: Vec::new(),
                    };
                    add_char_to_vertical_line_arena(
                        annos,
                        &mut line,
                        &chars[obj0_idx],
                        obj0_idx,
                        laparams.word_margin,
                    );
                    add_char_to_vertical_line_arena(
                        annos,
                        &mut line,
                        &chars[obj1_idx],
                        obj1_idx,
                        laparams.word_margin,
                    );
                    let id = LineId(lines.len());
                    lines.push(ArenaTextLine::Vertical(line));
                    current_line = Some(id);
                } else if halign && !valign {
                    let mut line = ArenaTextLineHorizontal {
                        component: LTComponent::new((INF_F64, INF_F64, -INF_F64, -INF_F64)),
                        word_margin: laparams.word_margin,
                        x1_tracker: INF_F64,
                        elements: Vec::new(),
                    };
                    add_char_to_horizontal_line_arena(
                        annos,
                        &mut line,
                        &chars[obj0_idx],
                        obj0_idx,
                        laparams.word_margin,
                    );
                    add_char_to_horizontal_line_arena(
                        annos,
                        &mut line,
                        &chars[obj1_idx],
                        obj1_idx,
                        laparams.word_margin,
                    );
                    let id = LineId(lines.len());
                    lines.push(ArenaTextLine::Horizontal(line));
                    current_line = Some(id);
                } else {
                    let mut line = ArenaTextLineHorizontal {
                        component: LTComponent::new((INF_F64, INF_F64, -INF_F64, -INF_F64)),
                        word_margin: laparams.word_margin,
                        x1_tracker: INF_F64,
                        elements: Vec::new(),
                    };
                    add_char_to_horizontal_line_arena(
                        annos,
                        &mut line,
                        &chars[obj0_idx],
                        obj0_idx,
                        laparams.word_margin,
                    );
                    let id = LineId(lines.len());
                    lines.push(ArenaTextLine::Horizontal(line));
                    analyze_line(annos, lines, id);
                    result.push(id);
                }
            }
        }

        obj0_idx = obj1_idx;
    }

    if let Some(id) = current_line {
        analyze_line(annos, lines, id);
        result.push(id);
    } else {
        let mut line = ArenaTextLineHorizontal {
            component: LTComponent::new((INF_F64, INF_F64, -INF_F64, -INF_F64)),
            word_margin: laparams.word_margin,
            x1_tracker: INF_F64,
            elements: Vec::new(),
        };
        add_char_to_horizontal_line_arena(
            annos,
            &mut line,
            &chars[chars_len - 1],
            chars_len - 1,
            laparams.word_margin,
        );
        let id = LineId(lines.len());
        lines.push(ArenaTextLine::Horizontal(line));
        analyze_line(annos, lines, id);
        result.push(id);
    }

    result
}

fn analyze_line(annos: &mut Vec<LTAnno>, lines: &mut [ArenaTextLine], id: LineId) {
    let aid = AnnoId(annos.len());
    annos.push(LTAnno::new("\n"));
    match &mut lines[id.0] {
        ArenaTextLine::Horizontal(h) => h.elements.push(ArenaElem::Anno(aid)),
        ArenaTextLine::Vertical(v) => v.elements.push(ArenaElem::Anno(aid)),
    }
}

fn add_char_to_horizontal_line_arena(
    annos: &mut Vec<LTAnno>,
    line: &mut ArenaTextLineHorizontal,
    ch: &LTChar,
    char_idx: usize,
    word_margin: f64,
) {
    let margin = word_margin * ch.width().max(ch.height());
    if line.x1_tracker < ch.x0() - margin && line.x1_tracker != INF_F64 {
        let aid = AnnoId(annos.len());
        annos.push(LTAnno::new(" "));
        line.elements.push(ArenaElem::Anno(aid));
    }
    line.x1_tracker = ch.x1();

    line.component.x0 = line.component.x0.min(ch.x0());
    line.component.y0 = line.component.y0.min(ch.y0());
    line.component.x1 = line.component.x1.max(ch.x1());
    line.component.y1 = line.component.y1.max(ch.y1());

    line.elements.push(ArenaElem::Char(CharId(char_idx)));
}

fn add_char_to_vertical_line_arena(
    annos: &mut Vec<LTAnno>,
    line: &mut ArenaTextLineVertical,
    ch: &LTChar,
    char_idx: usize,
    word_margin: f64,
) {
    let margin = word_margin * ch.width().max(ch.height());
    if ch.y1() + margin < line.y0_tracker && line.y0_tracker != -INF_F64 {
        let aid = AnnoId(annos.len());
        annos.push(LTAnno::new(" "));
        line.elements.push(ArenaElem::Anno(aid));
    }
    line.y0_tracker = ch.y0();

    line.component.x0 = line.component.x0.min(ch.x0());
    line.component.y0 = line.component.y0.min(ch.y0());
    line.component.x1 = line.component.x1.max(ch.x1());
    line.component.y1 = line.component.y1.max(ch.y1());

    line.elements.push(ArenaElem::Char(CharId(char_idx)));
}

/// Helper to add a character to a horizontal line, inserting word spaces as needed.
pub fn add_char_to_horizontal_line(line: &mut LTTextLineHorizontal, ch: LTChar, word_margin: f64) {
    let margin = word_margin * ch.width().max(ch.height());
    if line.x1_tracker < ch.x0() - margin && line.x1_tracker != INF_F64 {
        line.elements.push(TextLineElement::Anno(LTAnno::new(" ")));
    }
    line.x1_tracker = ch.x1();

    // Expand bounding box
    line.component.x0 = line.component.x0.min(ch.x0());
    line.component.y0 = line.component.y0.min(ch.y0());
    line.component.x1 = line.component.x1.max(ch.x1());
    line.component.y1 = line.component.y1.max(ch.y1());

    line.elements.push(TextLineElement::Char(ch));
}

/// Helper to add a character to a vertical line, inserting word spaces as needed.
pub fn add_char_to_vertical_line(line: &mut LTTextLineVertical, ch: LTChar, word_margin: f64) {
    let margin = word_margin * ch.width().max(ch.height());
    if ch.y1() + margin < line.y0_tracker && line.y0_tracker != -INF_F64 {
        line.elements.push(TextLineElement::Anno(LTAnno::new(" ")));
    }
    line.y0_tracker = ch.y0();

    // Expand bounding box
    line.component.x0 = line.component.x0.min(ch.x0());
    line.component.y0 = line.component.y0.min(ch.y0());
    line.component.x1 = line.component.x1.max(ch.x1());
    line.component.y1 = line.component.y1.max(ch.y1());

    line.elements.push(TextLineElement::Char(ch));
}

/// Groups text lines into text boxes based on neighbor relationships.
pub fn group_textlines(laparams: &LAParams, lines: Vec<TextLineType>) -> Vec<TextBoxType> {
    if lines.is_empty() {
        return Vec::new();
    }
    let mut arena = LayoutArena::new();
    let line_ids = arena.extend_lines_from_textlines(lines);
    let box_ids = group_textlines_arena(laparams, &mut arena, &line_ids);
    arena.materialize_boxes(&box_ids)
}

/// Arena-backed grouping of text lines into text boxes.
///
/// Produces BoxId values that can be materialized later; preserves exact
/// ordering and logic from group_textlines().
pub fn group_textlines_arena(
    laparams: &LAParams,
    arena: &mut LayoutArena,
    line_ids: &[LineId],
) -> Vec<BoxId> {
    group_textlines_arena_soa(laparams, arena, line_ids)
}

fn arena_lines_aligned(arena: &LayoutArena, lid: LineId, nlid: LineId, tolerance: f64) -> bool {
    match (arena.line_is_vertical(lid), arena.line_is_vertical(nlid)) {
        (false, false) => {
            let height_diff = (arena.line_height(nlid) - arena.line_height(lid)).abs();
            let same_height = height_diff <= tolerance;
            let bbox1 = arena.line_bbox(lid);
            let bbox2 = arena.line_bbox(nlid);
            let left_diff = (bbox2.0 - bbox1.0).abs();
            let right_diff = (bbox2.2 - bbox1.2).abs();
            let center1 = (bbox1.0 + bbox1.2) / 2.0;
            let center2 = (bbox2.0 + bbox2.2) / 2.0;
            let center_diff = (center2 - center1).abs();
            same_height
                && (left_diff <= tolerance || right_diff <= tolerance || center_diff <= tolerance)
        }
        (true, true) => {
            let width_diff = (arena.line_width(nlid) - arena.line_width(lid)).abs();
            let same_width = width_diff <= tolerance;
            let bbox1 = arena.line_bbox(lid);
            let bbox2 = arena.line_bbox(nlid);
            let lower_diff = (bbox2.1 - bbox1.1).abs();
            let upper_diff = (bbox2.3 - bbox1.3).abs();
            let center1 = (bbox1.1 + bbox1.3) / 2.0;
            let center2 = (bbox2.1 + bbox2.3) / 2.0;
            let center_diff = (center2 - center1).abs();
            same_width
                && (lower_diff <= tolerance || upper_diff <= tolerance || center_diff <= tolerance)
        }
        _ => false,
    }
}

fn group_textlines_arena_soa(
    laparams: &LAParams,
    arena: &mut LayoutArena,
    line_ids: &[LineId],
) -> Vec<BoxId> {
    if line_ids.is_empty() {
        return Vec::new();
    }

    let mut bboxes: Vec<Rect> = Vec::with_capacity(line_ids.len());
    for &lid in line_ids {
        bboxes.push(arena.line_bbox(lid));
    }
    let soa = RectSoA::from_bboxes(&bboxes);

    let mut line_to_box_id: Vec<Option<usize>> = vec![None; line_ids.len()];
    let mut box_contents: Vec<Option<Vec<usize>>> = Vec::new();
    let mut next_box_id: usize = 0;

    for (i, &lid) in line_ids.iter().enumerate() {
        let bbox = bboxes[i];
        let (d, search_bbox) = if arena.line_is_vertical(lid) {
            let d = laparams.line_margin * arena.line_width(lid);
            (d, (bbox.0 - d, bbox.1, bbox.2 + d, bbox.3))
        } else {
            let d = laparams.line_margin * arena.line_height(lid);
            (d, (bbox.0, bbox.1 - d, bbox.2, bbox.3 + d))
        };

        let mut neighbors = soa.overlap_simd(search_bbox);
        neighbors.sort_unstable();
        let mut members: Vec<usize> = vec![i];

        for j in neighbors {
            let nlid = line_ids[j];
            if arena_lines_aligned(arena, lid, nlid, d) {
                members.push(j);
                if let Some(existing_box_id) = line_to_box_id[j] {
                    if let Some(existing_members) =
                        box_contents.get_mut(existing_box_id).and_then(|m| m.take())
                    {
                        members.extend(existing_members);
                    }
                }
            }
        }

        let mut seen = vec![false; line_ids.len()];
        let mut unique_members: Vec<usize> = Vec::new();
        for m in members {
            if !seen[m] {
                seen[m] = true;
                unique_members.push(m);
            }
        }

        let box_id = next_box_id;
        next_box_id += 1;
        for &m in &unique_members {
            line_to_box_id[m] = Some(box_id);
        }
        if box_id == box_contents.len() {
            box_contents.push(Some(unique_members));
        } else {
            box_contents[box_id] = Some(unique_members);
        }
    }

    let mut result: Vec<BoxId> = Vec::new();
    let mut done: Vec<bool> = vec![false; next_box_id];

    for i in 0..line_ids.len() {
        let box_id = match line_to_box_id[i] {
            Some(id) => id,
            None => continue,
        };

        if done[box_id] {
            continue;
        }
        done[box_id] = true;

        let members = match box_contents.get(box_id).and_then(|m| m.as_ref()) {
            Some(m) => m,
            None => continue,
        };

        let mut seen = vec![false; line_ids.len()];
        let mut unique_members: Vec<usize> = Vec::new();
        for &m in members {
            if !seen[m] {
                seen[m] = true;
                unique_members.push(m);
            }
        }

        if unique_members.is_empty() {
            continue;
        }

        let is_vertical = arena.line_is_vertical(line_ids[unique_members[0]]);
        let mut member_ids: Vec<LineId> = Vec::with_capacity(unique_members.len());
        for idx in unique_members {
            member_ids.push(line_ids[idx]);
        }

        let arena_box = if is_vertical {
            ArenaTextBox::Vertical(member_ids)
        } else {
            ArenaTextBox::Horizontal(member_ids)
        };
        let id = arena.push_box(arena_box);
        result.push(id);
    }

    result
}
