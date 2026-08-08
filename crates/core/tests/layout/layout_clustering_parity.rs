use bolivar_core::layout::analysis::group_textboxes_exact;
use bolivar_core::layout::params::LAParams;
use bolivar_core::layout::types::{LTTextBoxHorizontal, TextBoxType};

#[test]
fn exact_grouping_minimal_fixture() {
    let boxes = vec![TextBoxType::Horizontal(LTTextBoxHorizontal::new())];
    let groups = group_textboxes_exact(&LAParams::default(), &boxes);

    assert_eq!(groups.len(), 1);
}
