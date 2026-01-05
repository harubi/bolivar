use bolivar_core::layout::analysis::{group_objects, group_objects_arena};
use bolivar_core::layout::arena::LayoutArena;
use bolivar_core::layout::elements::LTChar;
use bolivar_core::layout::params::LAParams;

#[test]
fn arena_group_objects_parity() {
    let chars = vec![
        LTChar::new((0.0, 0.0, 10.0, 10.0), "A", "F1", 10.0, true, 10.0),
        LTChar::new((12.0, 0.0, 22.0, 10.0), "B", "F1", 10.0, true, 10.0),
    ];
    let laparams = LAParams::default();

    let baseline = group_objects(&laparams, &chars);

    let mut arena = LayoutArena::new();
    for ch in &chars {
        arena.push_char(ch.clone());
    }

    let lines = group_objects_arena(&laparams, &mut arena);
    let materialized = arena.materialize_lines(&lines);

    assert_eq!(baseline, materialized);
}
