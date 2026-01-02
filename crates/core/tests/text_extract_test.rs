use bolivar_core::layout::{LTChar, LTItem, LTPage};
use bolivar_core::table::{
    PageGeometry, TextSettings, extract_text_from_ltpage, extract_words_from_ltpage,
};

#[test]
fn test_extract_words_simple() {
    let mut page = LTPage::new(1, (0.0, 0.0, 200.0, 100.0), 0.0);
    page.add(LTItem::Char(LTChar::new(
        (0.0, 0.0, 10.0, 10.0),
        "A",
        "F1",
        10.0,
        true,
        0.0,
    )));
    page.add(LTItem::Char(LTChar::new(
        (12.0, 0.0, 22.0, 10.0),
        "B",
        "F1",
        10.0,
        true,
        0.0,
    )));

    let geom = PageGeometry {
        page_bbox: (0.0, 0.0, 200.0, 100.0),
        mediabox: (0.0, 0.0, 200.0, 100.0),
        initial_doctop: 0.0,
        force_crop: false,
    };
    let words = extract_words_from_ltpage(&page, &geom, TextSettings::default());
    assert_eq!(words[0].text, "AB");
}

#[test]
fn test_extract_text_simple() {
    let mut page = LTPage::new(1, (0.0, 0.0, 200.0, 100.0), 0.0);
    page.add(LTItem::Char(LTChar::new(
        (0.0, 0.0, 10.0, 10.0),
        "A",
        "F1",
        10.0,
        true,
        0.0,
    )));
    page.add(LTItem::Char(LTChar::new(
        (12.0, 0.0, 22.0, 10.0),
        "B",
        "F1",
        10.0,
        true,
        0.0,
    )));

    let geom = PageGeometry {
        page_bbox: (0.0, 0.0, 200.0, 100.0),
        mediabox: (0.0, 0.0, 200.0, 100.0),
        initial_doctop: 0.0,
        force_crop: false,
    };
    let text = extract_text_from_ltpage(&page, &geom, TextSettings::default());
    assert_eq!(text, "AB");
}
