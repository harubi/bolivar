//! Character-to-line and line-to-box grouping algorithms.
//!
//! Contains group_objects() for grouping characters into text lines,
//! and group_textlines() for grouping text lines into text boxes.

#[cfg(test)]
use crate::utils::{HasBBox, Plane};
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
    let mut result = Vec::new();
    if objs.is_empty() {
        return result;
    }

    let mut obj_iter = objs.iter().peekable();
    let mut current_line: Option<TextLineType> = None;

    // Get first object
    let mut obj0 = match obj_iter.next() {
        Some(o) => o,
        None => return result,
    };

    for obj1 in obj_iter {
        // Check horizontal alignment:
        //   +------+ - - -
        //   | obj0 | - - +------+   -
        //   |      |     | obj1 |   | (line_overlap)
        //   +------+ - - |      |   -
        //          - - - +------+
        //          |<--->|
        //        (char_margin)
        let halign = obj0.is_voverlap(obj1)
            && obj0.height().min(obj1.height()) * laparams.line_overlap < obj0.voverlap(obj1)
            && obj0.hdistance(obj1) < obj0.width().max(obj1.width()) * laparams.char_margin;

        // Check vertical alignment:
        //   +------+
        //   | obj0 |
        //   |      |
        //   +------+ - - -
        //     |    |     | (char_margin)
        //     +------+ - -
        //     | obj1 |
        //     |      |
        //     +------+
        //     |<-->|
        //   (line_overlap)
        let valign = laparams.detect_vertical
            && obj0.is_hoverlap(obj1)
            && obj0.width().min(obj1.width()) * laparams.line_overlap < obj0.hoverlap(obj1)
            && obj0.vdistance(obj1) < obj0.height().max(obj1.height()) * laparams.char_margin;

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

        obj0 = obj1;
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
            add_char_to_horizontal_line(&mut line, obj0.clone(), laparams.word_margin);
            line.analyze();
            result.push(TextLineType::Horizontal(line));
        }
    }

    result
}

#[cfg(test)]
mod arena_soa_tests {
    use super::*;

    fn hline(bbox: Rect) -> TextLineType {
        let mut line = LTTextLineHorizontal::new(0.1);
        line.set_bbox(bbox);
        TextLineType::Horizontal(line)
    }

    fn box_signatures(boxes: &[TextBoxType]) -> Vec<(bool, Rect)> {
        boxes
            .iter()
            .map(|b| match b {
                TextBoxType::Horizontal(h) => (false, h.bbox()),
                TextBoxType::Vertical(v) => (true, v.bbox()),
            })
            .collect()
    }

    #[test]
    fn arena_soa_matches_plane() {
        let laparams = LAParams::default();
        let lines = vec![
            hline((0.0, 0.0, 10.0, 2.0)),
            hline((0.0, 2.5, 10.0, 4.5)),
            hline((20.0, 0.0, 30.0, 2.0)),
            hline((0.0, 10.0, 10.0, 12.0)),
        ];

        let mut arena_plane = LayoutArena::new();
        let plane_ids = arena_plane.extend_lines_from_textlines(lines.clone());
        let plane_boxes = group_textlines_arena_plane(&laparams, &mut arena_plane, &plane_ids);
        let plane_materialized = arena_plane.materialize_boxes(&plane_boxes);

        let mut arena_soa = LayoutArena::new();
        let soa_ids = arena_soa.extend_lines_from_textlines(lines);
        let soa_boxes = group_textlines_arena_soa(&laparams, &mut arena_soa, &soa_ids);
        let soa_materialized = arena_soa.materialize_boxes(&soa_boxes);

        assert_eq!(
            box_signatures(&plane_materialized),
            box_signatures(&soa_materialized)
        );
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

#[cfg(test)]
fn group_textlines_arena_plane(
    laparams: &LAParams,
    arena: &mut LayoutArena,
    line_ids: &[LineId],
) -> Vec<BoxId> {
    if line_ids.is_empty() {
        return Vec::new();
    }

    #[derive(Clone)]
    struct LineRef {
        bbox: Rect,
    }

    impl HasBBox for LineRef {
        fn x0(&self) -> f64 {
            self.bbox.0
        }
        fn y0(&self) -> f64 {
            self.bbox.1
        }
        fn x1(&self) -> f64 {
            self.bbox.2
        }
        fn y1(&self) -> f64 {
            self.bbox.3
        }
    }

    let mut min_x0 = INF_F64;
    let mut min_y0 = INF_F64;
    let mut max_x1 = -INF_F64;
    let mut max_y1 = -INF_F64;

    let mut refs: Vec<LineRef> = Vec::with_capacity(line_ids.len());
    for &lid in line_ids {
        let bbox = arena.line_bbox(lid);
        min_x0 = min_x0.min(bbox.0);
        min_y0 = min_y0.min(bbox.1);
        max_x1 = max_x1.max(bbox.2);
        max_y1 = max_y1.max(bbox.3);
        refs.push(LineRef { bbox });
    }

    let plane_bbox = (min_x0 - 1.0, min_y0 - 1.0, max_x1 + 1.0, max_y1 + 1.0);
    let mut plane: Plane<LineRef> = Plane::new(plane_bbox, 1);
    plane.extend(refs.iter().cloned());

    let mut line_to_box_id: Vec<Option<usize>> = vec![None; line_ids.len()];
    let mut box_contents: Vec<Option<Vec<usize>>> = Vec::new();
    let mut next_box_id: usize = 0;

    for (i, line_ref) in refs.iter().enumerate() {
        let lid = line_ids[i];
        let (d, search_bbox) = if arena.line_is_vertical(lid) {
            let d = laparams.line_margin * arena.line_width(lid);
            (
                d,
                (
                    line_ref.bbox.0 - d,
                    line_ref.bbox.1,
                    line_ref.bbox.2 + d,
                    line_ref.bbox.3,
                ),
            )
        } else {
            let d = laparams.line_margin * arena.line_height(lid);
            (
                d,
                (
                    line_ref.bbox.0,
                    line_ref.bbox.1 - d,
                    line_ref.bbox.2,
                    line_ref.bbox.3 + d,
                ),
            )
        };

        let neighbors = plane.find_with_indices(search_bbox);
        let mut members: Vec<usize> = vec![i];

        for (j, _neighbor) in neighbors {
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
