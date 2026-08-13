//! Character-to-line and line-to-box grouping algorithms.
//!
//! Contains `group_objects()` (characters → text lines) and `group_textlines()`
//! (text lines → text boxes).
//! Distinct from [`super::clustering`], which is a best-first hierarchical
//! merge over already-formed boxes downstream of this stage.

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

fn objects_aligned(laparams: &LAParams, obj0: &LTChar, obj1: &LTChar) -> (bool, bool) {
    let ax0 = obj0.x0();
    let ay0 = obj0.y0();
    let ax1 = obj0.x1();
    let ay1 = obj0.y1();
    let bx0 = obj1.x0();
    let by0 = obj1.y0();
    let bx1 = obj1.x1();
    let by1 = obj1.y1();

    let is_voverlap = by0 <= ay1 && ay0 <= by1;
    let is_hoverlap = bx0 <= ax1 && ax0 <= bx1;
    let vmin = (ay0 - by1).abs().min((ay1 - by0).abs());
    let hmin = (ax0 - bx1).abs().min((ax1 - bx0).abs());
    let voverlap = if is_voverlap { vmin } else { 0.0 };
    let vdistance = if is_voverlap { 0.0 } else { vmin };
    let hoverlap = if is_hoverlap { hmin } else { 0.0 };
    let hdistance = if is_hoverlap { 0.0 } else { hmin };
    let width0 = ax1 - ax0;
    let width1 = bx1 - bx0;
    let height0 = ay1 - ay0;
    let height1 = by1 - by0;

    let halign = is_voverlap
        && height0.min(height1) * laparams.line_overlap < voverlap
        && hdistance < width0.max(width1) * laparams.char_margin;
    let valign = laparams.detect_vertical
        && is_hoverlap
        && width0.min(width1) * laparams.line_overlap < hoverlap
        && vdistance < height0.max(height1) * laparams.char_margin;
    (halign, valign)
}

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
    let mut result = Vec::new();

    let mut current_line: Option<TextLineType> = None;
    let mut obj0_idx = 0usize;

    for obj1_idx in 1..objs.len() {
        let obj0 = &objs[obj0_idx];
        let obj1 = &objs[obj1_idx];
        let (halign, valign) = objects_aligned(laparams, obj0, obj1);

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

#[cfg(test)]
mod group_objects_tests {
    use super::*;

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
}

#[cfg(test)]
mod arena_grouping_tests {
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
    fn arena_grouping_expected_output() {
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
        let (halign, valign) = objects_aligned(laparams, obj0, obj1);

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
                        bidi: false,
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
                        bidi: false,
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
                        bidi: false,
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
            bidi: false,
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

    line.elements.push(TextLineElement::Char(Box::new(ch)));
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

    line.elements.push(TextLineElement::Char(Box::new(ch)));
}

/// Groups text lines into text boxes based on neighbor relationships.
pub fn group_textlines(laparams: &LAParams, lines: Vec<TextLineType>) -> Vec<TextBoxType> {
    if lines.is_empty() {
        return Vec::new();
    }
    let mut arena = LayoutArena::new();
    let line_ids = arena.extend_lines_from_textlines(lines);
    let box_ids = group_textlines_arena(laparams, &mut arena, &line_ids);
    arena.into_materialized(&box_ids, &[]).0
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
    group_textlines_arena_impl(laparams, arena, line_ids)
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

fn group_textlines_arena_impl(
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
    let mut line_to_box_id: Vec<Option<usize>> = vec![None; line_ids.len()];
    let mut box_contents: Vec<Option<Vec<usize>>> = Vec::new();
    let mut seen_generation = vec![0usize; line_ids.len()];
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

        let mut members: Vec<usize> = vec![i];
        for (j, neighbor_bbox) in bboxes.iter().enumerate() {
            if neighbor_bbox.0 < search_bbox.2
                && neighbor_bbox.2 > search_bbox.0
                && neighbor_bbox.1 < search_bbox.3
                && neighbor_bbox.3 > search_bbox.1
            {
                let nlid = line_ids[j];
                if arena_lines_aligned(arena, lid, nlid, d) {
                    members.push(j);
                    if let Some(existing_box_id) = line_to_box_id[j]
                        && let Some(existing_members) = box_contents
                            .get_mut(existing_box_id)
                            .and_then(|members| members.take())
                    {
                        members.extend(existing_members);
                    }
                }
            }
        }

        let generation = i + 1;
        let mut unique_members: Vec<usize> = Vec::new();
        for m in members {
            if seen_generation[m] != generation {
                seen_generation[m] = generation;
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

    for box_id in line_to_box_id.iter().take(line_ids.len()) {
        let box_id = match box_id {
            Some(id) => *id,
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
