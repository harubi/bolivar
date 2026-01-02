//! Tests for Rust table extraction (ported from pdfplumber table.py logic)

use bolivar_core::layout::{LTChar, LTItem, LTLine, LTPage};
use bolivar_core::table::{PageGeometry, TableSettings, extract_tables_from_ltpage};

#[test]
fn test_extract_tables_simple_grid() {
    // Page bbox in PDF coords (origin bottom-left)
    let mut page = LTPage::new(1, (0.0, 0.0, 200.0, 100.0), 0.0);

    // Horizontal lines (y = 0, 50, 100)
    let h0 = LTLine::new(
        1.0,
        (0.0, 0.0),
        (200.0, 0.0),
        true,
        false,
        false,
        None,
        None,
    );
    let h1 = LTLine::new(
        1.0,
        (0.0, 50.0),
        (200.0, 50.0),
        true,
        false,
        false,
        None,
        None,
    );
    let h2 = LTLine::new(
        1.0,
        (0.0, 100.0),
        (200.0, 100.0),
        true,
        false,
        false,
        None,
        None,
    );

    // Vertical lines (x = 0, 100, 200)
    let v0 = LTLine::new(
        1.0,
        (0.0, 0.0),
        (0.0, 100.0),
        true,
        false,
        false,
        None,
        None,
    );
    let v1 = LTLine::new(
        1.0,
        (100.0, 0.0),
        (100.0, 100.0),
        true,
        false,
        false,
        None,
        None,
    );
    let v2 = LTLine::new(
        1.0,
        (200.0, 0.0),
        (200.0, 100.0),
        true,
        false,
        false,
        None,
        None,
    );

    page.add(LTItem::Line(h0));
    page.add(LTItem::Line(h1));
    page.add(LTItem::Line(h2));
    page.add(LTItem::Line(v0));
    page.add(LTItem::Line(v1));
    page.add(LTItem::Line(v2));

    // Add one char per cell:
    // Cell (0,0) top-left in pdfplumber coords corresponds to y in PDF coords 50..100
    let a = LTChar::new((10.0, 60.0, 20.0, 90.0), "A", "F1", 10.0, true, 10.0);
    let b = LTChar::new((110.0, 60.0, 120.0, 90.0), "B", "F1", 10.0, true, 10.0);
    let c = LTChar::new((10.0, 10.0, 20.0, 40.0), "C", "F1", 10.0, true, 10.0);
    let d = LTChar::new((110.0, 10.0, 120.0, 40.0), "D", "F1", 10.0, true, 10.0);

    page.add(LTItem::Char(a));
    page.add(LTItem::Char(b));
    page.add(LTItem::Char(c));
    page.add(LTItem::Char(d));

    let geom = PageGeometry {
        page_bbox: (0.0, 0.0, 200.0, 100.0),
        mediabox: (0.0, 0.0, 200.0, 100.0),
        initial_doctop: 0.0,
        force_crop: false,
    };

    let tables = extract_tables_from_ltpage(&page, &geom, &TableSettings::default());
    assert_eq!(tables.len(), 1);
    assert_eq!(
        tables[0],
        vec![
            vec![Some("A".to_string()), Some("B".to_string())],
            vec![Some("C".to_string()), Some("D".to_string())],
        ]
    );
}
