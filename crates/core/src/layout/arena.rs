use crate::layout::types::{
    LTAnno, LTChar, LTComponent, LTTextBoxHorizontal, LTTextBoxVertical, LTTextLineHorizontal,
    LTTextLineVertical, TextBoxType, TextLineElement, TextLineType,
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

#[derive(Debug, Clone)]
pub enum ArenaTextBox {
    Horizontal(Vec<LineId>),
    Vertical(Vec<LineId>),
}

#[derive(Default)]
pub struct LayoutArena {
    pub(crate) chars: Vec<LTChar>,
    pub(crate) annos: Vec<LTAnno>,
    pub(crate) lines: Vec<ArenaTextLine>,
    pub(crate) boxes: Vec<ArenaTextBox>,
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
        } = self;
        let mut chars: Vec<_> = chars.into_iter().map(Some).collect();
        let mut annos: Vec<_> = annos.into_iter().map(Some).collect();
        let mut lines: Vec<_> = lines
            .into_iter()
            .map(|line| Some(Self::materialize_owned_line(line, &mut chars, &mut annos)))
            .collect();

        let materialized_boxes = box_ids
            .iter()
            .map(|id| Self::materialize_owned_box(&boxes[id.0], &mut lines))
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
                TextBoxType::Horizontal(tb)
            }
            ArenaTextBox::Vertical(lines) => {
                let mut tb = LTTextBoxVertical::with_capacity(lines.len());
                for lid in lines {
                    if let TextLineType::Vertical(line) = self.materialize_line(*lid) {
                        tb.add(line);
                    }
                }
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
                TextBoxType::Vertical(text_box)
            }
        }
    }

    pub fn line_bbox(&self, id: LineId) -> Rect {
        match &self.lines[id.0] {
            ArenaTextLine::Horizontal(h) => h.component.bbox(),
            ArenaTextLine::Vertical(v) => v.component.bbox(),
        }
    }

    pub fn line_width(&self, id: LineId) -> f64 {
        match &self.lines[id.0] {
            ArenaTextLine::Horizontal(h) => h.component.width(),
            ArenaTextLine::Vertical(v) => v.component.width(),
        }
    }

    pub fn line_height(&self, id: LineId) -> f64 {
        match &self.lines[id.0] {
            ArenaTextLine::Horizontal(h) => h.component.height(),
            ArenaTextLine::Vertical(v) => v.component.height(),
        }
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
