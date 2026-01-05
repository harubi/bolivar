use bolivar_core::layout::analysis::group_textboxes_exact;
use bolivar_core::layout::elements::{LTTextBoxHorizontal, TextBoxType};
use bolivar_core::layout::params::LAParams;

#[test]
fn exact_grouping_minimal_fixture() {
    let mut box1 = LTTextBoxHorizontal::new();
    let boxes = vec![TextBoxType::Horizontal(box1)];
    let groups = group_textboxes_exact(&LAParams::default(), &boxes);

    assert_eq!(groups.len(), 1);
}
