use crate::layout::bidi::{
    contains_rtl_text, reconstruct_textline_raw_text, reorder_text_for_output,
};
use crate::layout::types::{
    LTAnno, LTChar, LTComponent, LTItem, LTTextBox, LTTextBoxHorizontal, LTTextBoxVertical,
    LTTextGroup, LTTextLineHorizontal, LTTextLineVertical, TextBoxType, TextGroupElement,
    TextLineElement, TextLineType,
};
use crate::utils::Rect;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct CharId(pub usize);
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct AnnoId(pub usize);
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct LineId(pub usize);
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct BoxId(pub usize);
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
struct GroupId(usize);

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ArenaElem {
    Char(CharId),
    Anno(AnnoId),
}

#[derive(Debug, Clone)]
pub struct ArenaTextLineHorizontal {
    pub(crate) component: LTComponent,
    pub(crate) word_margin: f64,
    pub(crate) x1_tracker: f64,
    pub(crate) elements: Vec<ArenaElem>,
    pub(crate) bidi: bool,
}

impl ArenaTextLineHorizontal {
    pub fn new(
        component: LTComponent,
        word_margin: f64,
        x1_tracker: f64,
        elements: Vec<ArenaElem>,
    ) -> Self {
        Self {
            component,
            word_margin,
            x1_tracker,
            elements,
            bidi: false,
        }
    }

    fn new_with_bidi(
        component: LTComponent,
        word_margin: f64,
        x1_tracker: f64,
        elements: Vec<ArenaElem>,
        bidi: bool,
    ) -> Self {
        let mut line = Self::new(component, word_margin, x1_tracker, elements);
        line.bidi = bidi;
        line
    }
}

#[derive(Debug, Clone)]
pub struct ArenaTextLineVertical {
    pub(crate) component: LTComponent,
    pub(crate) word_margin: f64,
    pub(crate) y0_tracker: f64,
    pub(crate) elements: Vec<ArenaElem>,
    pub(crate) bidi: bool,
}

#[derive(Debug, Clone)]
pub enum ArenaTextLine {
    Horizontal(ArenaTextLineHorizontal),
    Vertical(ArenaTextLineVertical),
}

impl ArenaTextLine {
    fn component(&self) -> &LTComponent {
        match self {
            Self::Horizontal(line) => &line.component,
            Self::Vertical(line) => &line.component,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ArenaTextBox {
    Horizontal(Vec<LineId>),
    Vertical(Vec<LineId>),
}

#[derive(Debug, Clone, Default)]
pub struct LayoutArena {
    pub(crate) chars: Vec<LTChar>,
    pub(crate) annos: Vec<LTAnno>,
    pub(crate) lines: Vec<ArenaTextLine>,
    pub(crate) boxes: Vec<ArenaTextBox>,
    box_indices: Vec<i32>,
}

impl LayoutArena {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_char_capacity(capacity: usize) -> Self {
        Self {
            chars: Vec::with_capacity(capacity),
            ..Self::default()
        }
    }

    pub fn push_char(&mut self, ch: LTChar) -> CharId {
        let id = CharId(self.chars.len());
        self.chars.push(ch);
        id
    }

    pub fn get_char(&self, id: CharId) -> &LTChar {
        &self.chars[id.0]
    }

    pub fn push_anno(&mut self, anno: LTAnno) -> AnnoId {
        let id = AnnoId(self.annos.len());
        self.annos.push(anno);
        id
    }

    pub fn get_anno(&self, id: AnnoId) -> &LTAnno {
        &self.annos[id.0]
    }

    pub fn push_line(&mut self, line: ArenaTextLine) -> LineId {
        let id = LineId(self.lines.len());
        self.lines.push(line);
        id
    }

    pub fn push_textline(&mut self, line: TextLineType) -> LineId {
        match line {
            TextLineType::Horizontal(h) => {
                let LTTextLineHorizontal {
                    component,
                    word_margin,
                    x1_tracker,
                    elements,
                    bidi,
                } = h;
                let mut arena_elems = Vec::with_capacity(elements.len());
                for element in elements {
                    match element {
                        TextLineElement::Char(ch) => {
                            let id = self.push_char(*ch);
                            arena_elems.push(ArenaElem::Char(id));
                        }
                        TextLineElement::Anno(anno) => {
                            let id = self.push_anno(anno);
                            arena_elems.push(ArenaElem::Anno(id));
                        }
                    }
                }
                let arena_line = ArenaTextLine::Horizontal(ArenaTextLineHorizontal::new_with_bidi(
                    component,
                    word_margin,
                    x1_tracker,
                    arena_elems,
                    bidi,
                ));
                self.push_line(arena_line)
            }
            TextLineType::Vertical(v) => {
                let LTTextLineVertical {
                    component,
                    word_margin,
                    y0_tracker,
                    elements,
                    bidi,
                } = v;
                let mut arena_elems = Vec::with_capacity(elements.len());
                for element in elements {
                    match element {
                        TextLineElement::Char(ch) => {
                            let id = self.push_char(*ch);
                            arena_elems.push(ArenaElem::Char(id));
                        }
                        TextLineElement::Anno(anno) => {
                            let id = self.push_anno(anno);
                            arena_elems.push(ArenaElem::Anno(id));
                        }
                    }
                }
                let arena_line = ArenaTextLine::Vertical(ArenaTextLineVertical {
                    component,
                    word_margin,
                    y0_tracker,
                    elements: arena_elems,
                    bidi,
                });
                self.push_line(arena_line)
            }
        }
    }

    pub fn extend_lines_from_textlines(&mut self, lines: Vec<TextLineType>) -> Vec<LineId> {
        lines
            .into_iter()
            .map(|line| self.push_textline(line))
            .collect()
    }

    pub fn get_line(&self, id: LineId) -> &ArenaTextLine {
        &self.lines[id.0]
    }

    pub fn push_box(&mut self, b: ArenaTextBox) -> BoxId {
        let id = BoxId(self.boxes.len());
        self.boxes.push(b);
        self.box_indices.push(-1);
        id
    }

    pub fn materialize_lines(&self, ids: &[LineId]) -> Vec<TextLineType> {
        ids.iter().map(|id| self.materialize_line(*id)).collect()
    }

    pub fn materialize_boxes(&self, ids: &[BoxId]) -> Vec<TextBoxType> {
        ids.iter().map(|id| self.materialize_box(*id)).collect()
    }

    pub(crate) fn into_materialized(
        self,
        box_ids: &[BoxId],
        line_ids: &[LineId],
    ) -> (Vec<TextBoxType>, Vec<TextLineType>) {
        let Self {
            chars,
            annos,
            lines,
            boxes,
            box_indices,
        } = self;
        let mut chars: Vec<_> = chars.into_iter().map(Some).collect();
        let mut annos: Vec<_> = annos.into_iter().map(Some).collect();
        let mut lines: Vec<_> = lines
            .into_iter()
            .map(|line| Some(Self::materialize_owned_line(line, &mut chars, &mut annos)))
            .collect();

        let materialized_boxes = box_ids
            .iter()
            .map(|id| Self::materialize_owned_box(&boxes[id.0], box_indices[id.0], &mut lines))
            .collect();
        let materialized_lines = line_ids
            .iter()
            .map(|id| lines[id.0].take().expect("layout line must have one owner"))
            .collect();

        (materialized_boxes, materialized_lines)
    }

    pub fn analyze_line(&mut self, id: LineId) {
        let aid = self.push_anno(LTAnno::new("\n"));
        match &mut self.lines[id.0] {
            ArenaTextLine::Horizontal(h) => h.elements.push(ArenaElem::Anno(aid)),
            ArenaTextLine::Vertical(v) => v.elements.push(ArenaElem::Anno(aid)),
        }
    }

    fn materialize_line(&self, id: LineId) -> TextLineType {
        match &self.lines[id.0] {
            ArenaTextLine::Horizontal(h) => {
                let mut line = LTTextLineHorizontal::new(h.word_margin);
                line.component = h.component.clone();
                line.x1_tracker = h.x1_tracker;
                line.elements = h
                    .elements
                    .iter()
                    .map(|e| match e {
                        ArenaElem::Char(cid) => {
                            TextLineElement::Char(Box::new(self.chars[cid.0].clone()))
                        }
                        ArenaElem::Anno(aid) => TextLineElement::Anno(self.annos[aid.0].clone()),
                    })
                    .collect();
                line.bidi = h.bidi;
                TextLineType::Horizontal(line)
            }
            ArenaTextLine::Vertical(v) => {
                let mut line = LTTextLineVertical::new(v.word_margin);
                line.component = v.component.clone();
                line.y0_tracker = v.y0_tracker;
                line.elements = v
                    .elements
                    .iter()
                    .map(|e| match e {
                        ArenaElem::Char(cid) => {
                            TextLineElement::Char(Box::new(self.chars[cid.0].clone()))
                        }
                        ArenaElem::Anno(aid) => TextLineElement::Anno(self.annos[aid.0].clone()),
                    })
                    .collect();
                line.bidi = v.bidi;
                TextLineType::Vertical(line)
            }
        }
    }

    fn materialize_box(&self, id: BoxId) -> TextBoxType {
        match &self.boxes[id.0] {
            ArenaTextBox::Horizontal(lines) => {
                let mut tb = LTTextBoxHorizontal::with_capacity(lines.len());
                for lid in lines {
                    if let TextLineType::Horizontal(line) = self.materialize_line(*lid) {
                        tb.add(line);
                    }
                }
                tb.set_index(self.box_indices[id.0]);
                TextBoxType::Horizontal(tb)
            }
            ArenaTextBox::Vertical(lines) => {
                let mut tb = LTTextBoxVertical::with_capacity(lines.len());
                for lid in lines {
                    if let TextLineType::Vertical(line) = self.materialize_line(*lid) {
                        tb.add(line);
                    }
                }
                tb.set_index(self.box_indices[id.0]);
                TextBoxType::Vertical(tb)
            }
        }
    }

    fn materialize_owned_line(
        line: ArenaTextLine,
        chars: &mut [Option<LTChar>],
        annos: &mut [Option<LTAnno>],
    ) -> TextLineType {
        let mut take_elements = |elements: Vec<ArenaElem>| {
            elements
                .into_iter()
                .map(|element| match element {
                    ArenaElem::Char(id) => TextLineElement::Char(Box::new(
                        chars[id.0]
                            .take()
                            .expect("layout character must have one owner"),
                    )),
                    ArenaElem::Anno(id) => TextLineElement::Anno(
                        annos[id.0]
                            .take()
                            .expect("layout annotation must have one owner"),
                    ),
                })
                .collect()
        };

        match line {
            ArenaTextLine::Horizontal(line) => TextLineType::Horizontal(LTTextLineHorizontal {
                component: line.component,
                word_margin: line.word_margin,
                x1_tracker: line.x1_tracker,
                elements: take_elements(line.elements),
                bidi: line.bidi,
            }),
            ArenaTextLine::Vertical(line) => TextLineType::Vertical(LTTextLineVertical {
                component: line.component,
                word_margin: line.word_margin,
                y0_tracker: line.y0_tracker,
                elements: take_elements(line.elements),
                bidi: line.bidi,
            }),
        }
    }

    fn materialize_owned_box(
        text_box: &ArenaTextBox,
        index: i32,
        lines: &mut [Option<TextLineType>],
    ) -> TextBoxType {
        match text_box {
            ArenaTextBox::Horizontal(line_ids) => {
                let mut text_box = LTTextBoxHorizontal::with_capacity(line_ids.len());
                for id in line_ids {
                    if let TextLineType::Horizontal(line) =
                        lines[id.0].take().expect("layout line must have one owner")
                    {
                        text_box.add(line);
                    }
                }
                text_box.set_index(index);
                TextBoxType::Horizontal(text_box)
            }
            ArenaTextBox::Vertical(line_ids) => {
                let mut text_box = LTTextBoxVertical::with_capacity(line_ids.len());
                for id in line_ids {
                    if let TextLineType::Vertical(line) =
                        lines[id.0].take().expect("layout line must have one owner")
                    {
                        text_box.add(line);
                    }
                }
                text_box.set_index(index);
                TextBoxType::Vertical(text_box)
            }
        }
    }

    pub(crate) fn analyze_box(&mut self, id: BoxId) {
        let lines = &self.lines;
        match &mut self.boxes[id.0] {
            ArenaTextBox::Horizontal(line_ids) => line_ids.sort_by(|left, right| {
                let left_y1 = lines[left.0].component().y1;
                let right_y1 = lines[right.0].component().y1;
                right_y1
                    .partial_cmp(&left_y1)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            ArenaTextBox::Vertical(line_ids) => line_ids.sort_by(|left, right| {
                let left_x1 = lines[left.0].component().x1;
                let right_x1 = lines[right.0].component().x1;
                right_x1
                    .partial_cmp(&left_x1)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
        }
    }

    pub(crate) fn box_bbox(&self, id: BoxId) -> Rect {
        let line_ids = match &self.boxes[id.0] {
            ArenaTextBox::Horizontal(line_ids) | ArenaTextBox::Vertical(line_ids) => line_ids,
        };
        line_ids.iter().fold(
            (
                crate::utils::INF_F64,
                crate::utils::INF_F64,
                -crate::utils::INF_F64,
                -crate::utils::INF_F64,
            ),
            |bbox, line_id| {
                let line = self.line_bbox(*line_id);
                (
                    bbox.0.min(line.0),
                    bbox.1.min(line.1),
                    bbox.2.max(line.2),
                    bbox.3.max(line.3),
                )
            },
        )
    }

    pub(crate) fn box_is_vertical(&self, id: BoxId) -> bool {
        matches!(self.boxes[id.0], ArenaTextBox::Vertical(_))
    }

    pub(crate) fn box_proxy(&self, id: BoxId) -> TextBoxType {
        let bbox = self.box_bbox(id);
        let source_index = i32::try_from(id.0).expect("compact text box ID must fit in i32");
        if self.box_is_vertical(id) {
            TextBoxType::Vertical(LTTextBoxVertical::proxy(bbox, source_index))
        } else {
            TextBoxType::Horizontal(LTTextBoxHorizontal::proxy(bbox, source_index))
        }
    }

    pub(crate) fn set_box_index(&mut self, id: BoxId, index: i32) {
        self.box_indices[id.0] = index;
    }

    pub(crate) fn set_bidi(&mut self, bidi: bool) {
        for line in &mut self.lines {
            match line {
                ArenaTextLine::Horizontal(line) => line.bidi = bidi,
                ArenaTextLine::Vertical(line) => line.bidi = bidi,
            }
        }
    }

    fn element_text(&self, element: ArenaElem) -> &str {
        match element {
            ArenaElem::Char(id) => self.chars[id.0].get_text(),
            ArenaElem::Anno(id) => self.annos[id.0].get_text(),
        }
    }

    fn line_text(&self, id: LineId) -> String {
        let (elements, bidi, source_is_logical) = match &self.lines[id.0] {
            ArenaTextLine::Horizontal(line) => (
                line.elements.as_slice(),
                line.bidi,
                self.horizontal_source_is_logical(&line.elements),
            ),
            ArenaTextLine::Vertical(line) => (line.elements.as_slice(), line.bidi, true),
        };
        let capacity = elements
            .iter()
            .map(|element| self.element_text(*element).len())
            .sum();
        let mut raw_text = String::with_capacity(capacity);
        for element in elements {
            raw_text.push_str(self.element_text(*element));
        }

        if bidi {
            reconstruct_textline_raw_text(raw_text, source_is_logical)
        } else {
            reorder_text_for_output(&raw_text)
        }
    }

    fn horizontal_source_is_logical(&self, elements: &[ArenaElem]) -> bool {
        let mut previous_x = None;
        let mut increasing = 0;
        let mut decreasing = 0;
        for element in elements {
            let ArenaElem::Char(id) = element else {
                continue;
            };
            let character = &self.chars[id.0];
            if !contains_rtl_text(character.get_text()) {
                continue;
            }

            let bbox = character.bbox();
            let x = (bbox.0 + bbox.2) * 0.5;
            if let Some(previous_x) = previous_x {
                let difference: f64 = x - previous_x;
                if difference > 0.01 {
                    increasing += 1;
                } else if difference < -0.01 {
                    decreasing += 1;
                }
            }
            previous_x = Some(x);
        }
        decreasing > increasing
    }

    fn append_box_text(&self, id: BoxId, output: &mut String) {
        let line_ids = match &self.boxes[id.0] {
            ArenaTextBox::Horizontal(line_ids) | ArenaTextBox::Vertical(line_ids) => line_ids,
        };
        for line_id in line_ids {
            output.push_str(&self.line_text(*line_id));
        }
    }

    pub fn line_bbox(&self, id: LineId) -> Rect {
        self.lines[id.0].component().bbox()
    }

    pub fn line_width(&self, id: LineId) -> f64 {
        self.lines[id.0].component().width()
    }

    pub fn line_height(&self, id: LineId) -> f64 {
        self.lines[id.0].component().height()
    }

    pub fn line_is_vertical(&self, id: LineId) -> bool {
        matches!(self.lines[id.0], ArenaTextLine::Vertical(_))
    }

    pub fn line_is_empty(&self, id: LineId) -> bool {
        let line = &self.lines[id.0];
        let mut has_any = false;
        let mut has_non_ws = false;
        match line {
            ArenaTextLine::Horizontal(h) => {
                for e in &h.elements {
                    let s = match e {
                        ArenaElem::Char(cid) => self.chars[cid.0].get_text(),
                        ArenaElem::Anno(aid) => self.annos[aid.0].get_text(),
                    };
                    if !s.is_empty() {
                        has_any = true;
                    }
                    if s.chars().any(|c| !c.is_whitespace()) {
                        has_non_ws = true;
                        break;
                    }
                }
                h.component.is_empty() || (has_any && !has_non_ws)
            }
            ArenaTextLine::Vertical(v) => {
                for e in &v.elements {
                    let s = match e {
                        ArenaElem::Char(cid) => self.chars[cid.0].get_text(),
                        ArenaElem::Anno(aid) => self.annos[aid.0].get_text(),
                    };
                    if !s.is_empty() {
                        has_any = true;
                    }
                    if s.chars().any(|c| !c.is_whitespace()) {
                        has_non_ws = true;
                        break;
                    }
                }
                v.component.is_empty() || (has_any && !has_non_ws)
            }
        }
    }
}

#[derive(Debug, Clone)]
enum CompactGroupElement {
    Box(BoxId),
    Group(GroupId),
}

#[derive(Debug, Clone)]
struct CompactGroup {
    elements: Vec<CompactGroupElement>,
    vertical: bool,
}

/// Final page layout stored as contiguous arrays connected by stable IDs.
#[derive(Debug, Clone)]
pub(crate) struct CompactPageLayout {
    arena: LayoutArena,
    box_order: Vec<BoxId>,
    empty_lines: Vec<LineId>,
    other_items: Vec<LTItem>,
    groups: Vec<CompactGroup>,
    root_groups: Option<Vec<GroupId>>,
}

impl CompactPageLayout {
    pub(crate) fn new(
        arena: LayoutArena,
        box_order: Vec<BoxId>,
        empty_lines: Vec<LineId>,
        other_items: Vec<LTItem>,
        source_groups: Option<&[LTTextGroup]>,
    ) -> Self {
        let mut groups = Vec::new();
        let root_groups = source_groups.map(|source_groups| {
            source_groups
                .iter()
                .map(|group| Self::store_group(group, &box_order, &mut groups))
                .collect()
        });
        Self {
            arena,
            box_order,
            empty_lines,
            other_items,
            groups,
            root_groups,
        }
    }

    fn store_group(
        group: &LTTextGroup,
        box_order: &[BoxId],
        groups: &mut Vec<CompactGroup>,
    ) -> GroupId {
        let elements = group
            .elements()
            .iter()
            .map(|element| match element {
                TextGroupElement::Box(text_box) => {
                    let index = usize::try_from(text_box.index())
                        .expect("compact text box index must be non-negative");
                    CompactGroupElement::Box(
                        *box_order
                            .get(index)
                            .expect("compact text box index must be valid"),
                    )
                }
                TextGroupElement::Group(child) => {
                    CompactGroupElement::Group(Self::store_group(child, box_order, groups))
                }
            })
            .collect();
        let id = GroupId(groups.len());
        groups.push(CompactGroup {
            elements,
            vertical: group.is_vertical(),
        });
        id
    }

    pub(crate) fn materialize_items(&self) -> Vec<LTItem> {
        let mut items = Vec::with_capacity(
            self.box_order.len() + self.other_items.len() + self.empty_lines.len(),
        );
        items.extend(
            self.box_order
                .iter()
                .map(|id| LTItem::TextBox(self.arena.materialize_box(*id))),
        );
        items.extend(self.other_items.iter().cloned());
        items.extend(
            self.empty_lines
                .iter()
                .map(|id| LTItem::TextLine(self.arena.materialize_line(*id))),
        );
        items
    }

    pub(crate) fn append_text_boxes(&self, output: &mut String, first_box: &mut bool) {
        for id in &self.box_order {
            if !*first_box {
                output.push('\n');
            }
            *first_box = false;
            self.arena.append_box_text(*id, output);
        }
    }

    pub(crate) fn other_items(&self) -> &[LTItem] {
        &self.other_items
    }

    pub(crate) fn materialize_groups(&self) -> Vec<LTTextGroup> {
        let Some(root_groups) = &self.root_groups else {
            return Vec::new();
        };
        root_groups
            .iter()
            .map(|id| self.materialize_group(*id))
            .collect()
    }

    fn materialize_group(&self, id: GroupId) -> LTTextGroup {
        let group = &self.groups[id.0];
        let elements = group
            .elements
            .iter()
            .map(|element| match element {
                CompactGroupElement::Box(id) => {
                    TextGroupElement::Box(self.arena.materialize_box(*id))
                }
                CompactGroupElement::Group(id) => {
                    TextGroupElement::Group(Box::new(self.materialize_group(*id)))
                }
            })
            .collect();
        LTTextGroup::new(elements, group.vertical)
    }

    pub(crate) const fn has_groups(&self) -> bool {
        self.root_groups.is_some()
    }

    pub(crate) fn set_bidi(&mut self, bidi: bool) {
        self.arena.set_bidi(bidi);
    }

    #[cfg(test)]
    pub(crate) fn storage_counts(&self) -> (usize, usize, usize) {
        (
            self.arena.chars.len(),
            self.arena.lines.len(),
            self.arena.boxes.len(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::types::{
        LTAnno, LTChar, LTTextLine, LTTextLineHorizontal, TextLineElement, TextLineType,
    };

    #[test]
    fn arena_push_textline_roundtrip_preserves_text_and_bbox() {
        let mut line = LTTextLineHorizontal::new(0.1);
        line.set_bbox((0.0, 0.0, 10.0, 2.0));
        line.add_element(TextLineElement::Char(Box::new(LTChar::new(
            (0.0, 0.0, 1.0, 2.0),
            "a",
            "F1",
            10.0,
            true,
            1.0,
        ))));
        line.add_element(TextLineElement::Anno(LTAnno::new(" ")));

        let mut arena = LayoutArena::new();
        let id = arena.push_textline(TextLineType::Horizontal(line));
        let materialized = arena.materialize_lines(&[id]);
        assert_eq!(materialized.len(), 1);
        match &materialized[0] {
            TextLineType::Horizontal(h) => {
                assert_eq!(h.bbox(), (0.0, 0.0, 10.0, 2.0));
                assert_eq!(h.get_text(), "a ");
            }
            TextLineType::Vertical(_) => panic!("expected horizontal line"),
        }
    }

    #[test]
    fn owned_materialization_moves_character_color_storage() {
        let character = LTChar::builder((0.0, 0.0, 1.0, 2.0), "a", "F1", 10.0)
            .non_stroking_color(Some(vec![0.1, 0.2, 0.3]))
            .build();
        let color_ptr = character
            .non_stroking_color()
            .as_ref()
            .expect("test color")
            .as_ptr();

        let mut arena = LayoutArena::new();
        let character_id = arena.push_char(character);
        let line_id = arena.push_line(ArenaTextLine::Horizontal(ArenaTextLineHorizontal::new(
            LTComponent::new((0.0, 0.0, 1.0, 2.0)),
            0.1,
            1.0,
            vec![ArenaElem::Char(character_id)],
        )));
        let box_id = arena.push_box(ArenaTextBox::Horizontal(vec![line_id]));

        let (boxes, _) = arena.into_materialized(&[box_id], &[]);
        let TextBoxType::Horizontal(text_box) = &boxes[0] else {
            panic!("expected horizontal text box");
        };
        let TextLineElement::Char(character) =
            &text_box.iter().next().expect("text line").elements()[0]
        else {
            panic!("expected character");
        };
        let moved_color_ptr = character
            .non_stroking_color()
            .as_ref()
            .expect("materialized color")
            .as_ptr();

        assert_eq!(moved_color_ptr, color_ptr);
    }
}
