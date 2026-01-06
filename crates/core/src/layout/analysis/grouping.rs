//! Character-to-line and line-to-box grouping algorithms.
//!
//! Contains group_objects() for grouping characters into text lines,
//! and group_textlines() for grouping text lines into text boxes.

use crate::utils::{HasBBox, INF_F64, Plane, Rect, uniq};

use super::super::arena::{
    AnnoId, ArenaElem, ArenaTextBox, ArenaTextLine, ArenaTextLineHorizontal, ArenaTextLineVertical,
    BoxId, CharId, LayoutArena, LineId,
};
use super::super::params::LAParams;
use super::super::types::{
    LTAnno, LTChar, LTComponent, LTTextBox, LTTextBoxHorizontal, LTTextBoxVertical,
    LTTextLineHorizontal, LTTextLineVertical, TextBoxType, TextLineElement, TextLineType,
};

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

    // Compute bounding box that covers all lines (may be outside container bbox)
    let mut min_x0 = INF_F64;
    let mut min_y0 = INF_F64;
    let mut max_x1 = -INF_F64;
    let mut max_y1 = -INF_F64;

    for line in &lines {
        min_x0 = min_x0.min(line.x0());
        min_y0 = min_y0.min(line.y0());
        max_x1 = max_x1.max(line.x1());
        max_y1 = max_y1.max(line.y1());
    }

    // Create plane with expanded bbox
    let plane_bbox = (min_x0 - 1.0, min_y0 - 1.0, max_x1 + 1.0, max_y1 + 1.0);
    let mut plane: Plane<TextLineType> = Plane::new(plane_bbox, 1);

    // Add lines to plane (keep original lines with elements intact)
    for line in &lines {
        plane.add(line.clone());
    }
    let line_types = lines;

    // Group lines into boxes - MUST match Python's exact logic:
    // Python: boxes: Dict[LTTextLine, LTTextBox] = {}
    // Each line maps to its current box. When merging, ALL lines from
    // existing boxes are added to the new box.

    // line_to_box_id: maps line_index -> box_id (which box contains this line)
    // box_contents: maps box_id -> Vec<line_index> (lines in each box)
    let mut line_to_box_id: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::new();
    let mut box_contents: std::collections::HashMap<usize, Vec<usize>> =
        std::collections::HashMap::new();
    let mut next_box_id: usize = 0;

    for (i, line) in line_types.iter().enumerate() {
        // Use different search strategy for horizontal vs vertical text
        let (d, search_bbox) = match line {
            TextLineType::Horizontal(_) => {
                let d = laparams.line_margin * line.height();
                (d, (line.x0(), line.y0() - d, line.x1(), line.y1() + d))
            }
            TextLineType::Vertical(_) => {
                let d = laparams.line_margin * line.width();
                (d, (line.x0() - d, line.y0(), line.x1() + d, line.y1()))
            }
        };
        // Use find_with_indices to get (seq_index, neighbor) pairs
        // Since we added lines to plane in order, seq_index == line_types index
        let neighbors = plane.find_with_indices(search_bbox);

        // Start with current line
        let mut members: Vec<usize> = vec![i];

        for (j, neighbor) in neighbors {
            // Python uses NON-STRICT comparison (<= tolerance)
            // See layout.py:543-560 - _is_left_aligned_with, _is_same_height_as, etc.
            let is_aligned = match (line, neighbor) {
                (TextLineType::Horizontal(l1), TextLineType::Horizontal(l2)) => {
                    let tolerance = d;
                    let height_diff = (l2.height() - l1.height()).abs();
                    let same_height = height_diff <= tolerance; // Python: <=
                    let left_diff = (l2.x0() - l1.x0()).abs();
                    let left_aligned = left_diff <= tolerance; // Python: <=
                    let right_diff = (l2.x1() - l1.x1()).abs();
                    let right_aligned = right_diff <= tolerance; // Python: <=
                    let center1 = (l1.x0() + l1.x1()) / 2.0;
                    let center2 = (l2.x0() + l2.x1()) / 2.0;
                    let center_diff = (center2 - center1).abs();
                    let centrally_aligned = center_diff <= tolerance; // Python: <=
                    same_height && (left_aligned || right_aligned || centrally_aligned)
                }
                (TextLineType::Vertical(l1), TextLineType::Vertical(l2)) => {
                    let tolerance = d;
                    let same_width = (l2.width() - l1.width()).abs() <= tolerance; // Python: <=
                    let lower_aligned = (l2.y0() - l1.y0()).abs() <= tolerance; // Python: <=
                    let upper_aligned = (l2.y1() - l1.y1()).abs() <= tolerance; // Python: <=
                    let center1 = (l1.y0() + l1.y1()) / 2.0;
                    let center2 = (l2.y0() + l2.y1()) / 2.0;
                    let centrally_aligned = (center2 - center1).abs() <= tolerance; // Python: <=
                    same_width && (lower_aligned || upper_aligned || centrally_aligned)
                }
                _ => false,
            };

            if is_aligned {
                // j is the direct index from plane, no need to search by bbox!
                // Add neighbor to members
                members.push(j);
                // CRITICAL: If neighbor is already in a box, merge ALL lines from that box
                // This matches Python's: members.extend(boxes.pop(obj1))
                if let Some(&existing_box_id) = line_to_box_id.get(&j)
                    && let Some(existing_members) = box_contents.remove(&existing_box_id)
                {
                    members.extend(existing_members);
                }
            }
        }

        // Create new box with all members (matching Python: box = LTTextBox(); for obj in uniq(members): box.add(obj); boxes[obj] = box)
        let box_id = next_box_id;
        next_box_id += 1;

        let unique_members: Vec<usize> = uniq(members);
        for &m in &unique_members {
            line_to_box_id.insert(m, box_id);
        }
        box_contents.insert(box_id, unique_members);
    }

    // CRITICAL: Python iterates through original 'lines' in order and yields boxes
    // as their first line is encountered. We must do the same - NOT iterate the HashMap!
    let mut result: Vec<TextBoxType> = Vec::new();
    let mut done: Vec<bool> = vec![false; next_box_id];

    // Iterate through lines in ORIGINAL ORDER (like Python's "for line in lines:")
    for (i, _line) in line_types.iter().enumerate() {
        // Look up which box this line belongs to
        let box_id = match line_to_box_id.get(&i) {
            Some(&id) => id,
            None => continue,
        };

        // Skip if we've already processed this box
        if done[box_id] {
            continue;
        }
        done[box_id] = true;

        // Get all members of this box
        let member_indices = match box_contents.get(&box_id) {
            Some(members) => members,
            None => continue,
        };

        let unique_members: Vec<usize> = uniq(member_indices.clone());

        // Determine box type from first line in group
        if unique_members.is_empty() {
            continue;
        }
        let first_line = &line_types[unique_members[0]];
        let is_vertical = matches!(first_line, TextLineType::Vertical(_));

        if is_vertical {
            let mut textbox = LTTextBoxVertical::new();
            for idx in unique_members {
                if let TextLineType::Vertical(line) = &line_types[idx] {
                    textbox.add(line.clone());
                }
            }
            if !textbox.is_empty() {
                result.push(TextBoxType::Vertical(textbox));
            }
        } else {
            let mut textbox = LTTextBoxHorizontal::new();
            for idx in unique_members {
                if let TextLineType::Horizontal(line) = &line_types[idx] {
                    textbox.add(line.clone());
                }
            }
            if !textbox.is_empty() {
                result.push(TextBoxType::Horizontal(textbox));
            }
        }
    }

    result
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

    // Compute bounding box that covers all lines
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

    // Create plane with expanded bbox
    let plane_bbox = (min_x0 - 1.0, min_y0 - 1.0, max_x1 + 1.0, max_y1 + 1.0);
    let mut plane: Plane<LineRef> = Plane::new(plane_bbox, 1);
    plane.extend(refs.iter().cloned());

    // line_to_box_id: maps line_index -> box_id (which box contains this line)
    // box_contents: maps box_id -> Vec<line_index> (lines in each box)
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
            let is_aligned = match (arena.line_is_vertical(lid), arena.line_is_vertical(nlid)) {
                (false, false) => {
                    let tolerance = d;
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
                        && (left_diff <= tolerance
                            || right_diff <= tolerance
                            || center_diff <= tolerance)
                }
                (true, true) => {
                    let tolerance = d;
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
                        && (lower_diff <= tolerance
                            || upper_diff <= tolerance
                            || center_diff <= tolerance)
                }
                _ => false,
            };

            if is_aligned {
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

        // Deduplicate members while preserving order
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

    // Iterate through lines in ORIGINAL ORDER (like Python's "for line in lines:")
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

        // Deduplicate members while preserving order
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
