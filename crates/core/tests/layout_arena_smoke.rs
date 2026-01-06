use bolivar_core::layout::LTTextLine;
use bolivar_core::layout::arena::{
    ArenaElem, ArenaTextLine, ArenaTextLineHorizontal, CharId, LayoutArena, LineId,
};
use bolivar_core::layout::types::{LTChar, LTComponent, TextLineType};
use bolivar_core::utils::INF_F64;

#[test]
fn arena_push_char_and_line_materialize() {
    let mut arena = LayoutArena::new();
    let ch = LTChar::new((0.0, 0.0, 10.0, 10.0), "A", "F1", 10.0, true, 10.0);
    let cid: CharId = arena.push_char(ch.clone());

    let line = ArenaTextLine::Horizontal(ArenaTextLineHorizontal::new(
        LTComponent::new((INF_F64, INF_F64, -INF_F64, -INF_F64)),
        0.1,
        INF_F64,
        vec![ArenaElem::Char(cid)],
    ));
    let lid: LineId = arena.push_line(line);

    let materialized = arena.materialize_lines(&[lid]);
    assert_eq!(materialized.len(), 1);
    let text = match &materialized[0] {
        TextLineType::Horizontal(line) => line.get_text(),
        TextLineType::Vertical(line) => line.get_text(),
    };
    assert!(text.contains("A"));
}
