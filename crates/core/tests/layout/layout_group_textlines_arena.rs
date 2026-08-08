use bolivar_core::layout::LTTextBox;
use bolivar_core::layout::analysis::{
    group_objects, group_objects_arena, group_textlines, group_textlines_arena,
};
use bolivar_core::layout::arena::LayoutArena;
use bolivar_core::layout::params::LAParams;
use bolivar_core::layout::types::{LTChar, TextBoxType};
use bolivar_core::utils::HasBBox;

type BoxSignature = (bool, (f64, f64, f64, f64), String);

#[test]
fn arena_group_textlines_parity() {
    let chars = vec![
        LTChar::new((0.0, 0.0, 10.0, 10.0), "A", "F1", 10.0, true, 10.0),
        LTChar::new((12.0, 0.0, 22.0, 10.0), "B", "F1", 10.0, true, 10.0),
    ];
    let laparams = LAParams::default();

    let baseline_lines = group_objects(&laparams, &chars);
    let baseline_boxes = group_textlines(&laparams, baseline_lines);

    let mut arena = LayoutArena::new();
    for ch in &chars {
        arena.push_char(ch.clone());
    }
    let line_ids = group_objects_arena(&laparams, &mut arena);
    let box_ids = group_textlines_arena(&laparams, &mut arena, &line_ids);
    let materialized = arena.materialize_boxes(&box_ids);

    assert_eq!(
        box_signatures(&baseline_boxes),
        box_signatures(&materialized)
    );
}

fn box_signatures(boxes: &[TextBoxType]) -> Vec<BoxSignature> {
    boxes
        .iter()
        .map(|b| {
            let text = match b {
                TextBoxType::Horizontal(h) => h.get_text(),
                TextBoxType::Vertical(v) => v.get_text(),
            };
            (
                matches!(b, TextBoxType::Vertical(_)),
                (b.x0(), b.y0(), b.x1(), b.y1()),
                text,
            )
        })
        .collect()
}
