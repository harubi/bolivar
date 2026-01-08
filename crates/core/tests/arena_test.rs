use bolivar_core::arena::PageArena;
use bolivar_core::arena::types::{
    ArenaChar, ArenaCurve, ArenaItem, ArenaLine, ArenaPage, ArenaRect,
};
use bolivar_core::layout::LTItem;
use bolivar_core::utils::MATRIX_IDENTITY;

#[test]
fn test_page_arena_intern_and_reset() {
    let mut arena = PageArena::new();
    let a = arena.intern("Helvetica");
    let b = arena.intern("Helvetica");
    assert_eq!(a, b);
    arena.reset();
    let c = arena.intern("Helvetica");
    assert_eq!(arena.resolve(c), "Helvetica");
}

#[test]
fn test_materialize_char_roundtrip() {
    let mut arena = PageArena::new();
    let text = arena.intern("A");
    let fontname = arena.intern("F1");
    let ncolor = arena.intern_color(&[0.0]);
    let scolor = arena.intern_color(&[0.0]);
    let ch = arena.alloc_char(ArenaChar {
        bbox: (0.0, 0.0, 1.0, 1.0),
        text,
        fontname,
        size: 12.0,
        upright: true,
        adv: 1.0,
        matrix: MATRIX_IDENTITY,
        mcid: None,
        tag: None,
        ncs_name: None,
        scs_name: None,
        ncolor,
        scolor,
    });
    let lt = ch.materialize(&arena);
    assert_eq!(lt.get_text(), "A");
    assert_eq!(lt.fontname(), "F1");
}

#[test]
fn test_color_pool_dedup() {
    let mut arena = PageArena::new();
    let a = arena.intern_color(&[0.0, 1.0, 0.0]);
    let b = arena.intern_color(&[0.0, 1.0, 0.0]);
    assert_eq!(a, b);
}

#[test]
fn test_materialize_page_with_one_char() {
    let mut arena = PageArena::new();
    let text = arena.intern("A");
    let fontname = arena.intern("F1");
    let ncolor = arena.intern_color(&[0.0]);
    let scolor = arena.intern_color(&[0.0]);
    let ch = arena.alloc_char(ArenaChar {
        bbox: (0.0, 0.0, 1.0, 1.0),
        text,
        fontname,
        size: 12.0,
        upright: true,
        adv: 1.0,
        matrix: MATRIX_IDENTITY,
        mcid: None,
        tag: None,
        ncs_name: None,
        scs_name: None,
        ncolor,
        scolor,
    });
    let mut page = ArenaPage::new_in(&arena, 1, (0.0, 0.0, 100.0, 100.0));
    page.add(ArenaItem::Char(ch));
    let ltpage = page.materialize(&arena);
    assert_eq!(ltpage.iter().count(), 1);
}

#[test]
fn test_arena_page_new_in_uses_bump_vec() {
    let mut arena = PageArena::new();
    let text = arena.intern("A");
    let fontname = arena.intern("F1");
    let ncolor = arena.intern_color(&[0.0]);
    let scolor = arena.intern_color(&[0.0]);
    let ch = arena.alloc_char(ArenaChar {
        bbox: (0.0, 0.0, 1.0, 1.0),
        text,
        fontname,
        size: 12.0,
        upright: true,
        adv: 1.0,
        matrix: MATRIX_IDENTITY,
        mcid: None,
        tag: None,
        ncs_name: None,
        scs_name: None,
        ncolor,
        scolor,
    });
    let mut page = ArenaPage::new_in(&arena, 1, (0.0, 0.0, 100.0, 100.0));
    page.add(ArenaItem::Char(ch));
    let ltpage = page.materialize(&arena);
    assert_eq!(ltpage.iter().count(), 1);
}

#[test]
fn test_materialize_line_rect_curve() {
    let mut arena = PageArena::new();
    let color = arena.intern_color(&[0.0]);
    let mut page = ArenaPage::new_in(&arena, 1, (0.0, 0.0, 100.0, 100.0));
    page.add(ArenaItem::Line(ArenaLine {
        linewidth: 1.0,
        p0: (0.0, 0.0),
        p1: (10.0, 0.0),
        stroke: true,
        fill: false,
        evenodd: false,
        stroking_color: color,
        non_stroking_color: color,
        original_path: None,
        dashing_style: None,
        mcid: None,
        tag: None,
    }));
    page.add(ArenaItem::Rect(ArenaRect {
        linewidth: 1.0,
        bbox: (0.0, 0.0, 10.0, 10.0),
        stroke: true,
        fill: false,
        evenodd: false,
        stroking_color: color,
        non_stroking_color: color,
        original_path: None,
        dashing_style: None,
        mcid: None,
        tag: None,
    }));
    page.add(ArenaItem::Curve(ArenaCurve {
        linewidth: 1.0,
        pts: vec![(0.0, 0.0), (5.0, 10.0)],
        stroke: true,
        fill: false,
        evenodd: false,
        stroking_color: color,
        non_stroking_color: color,
        original_path: None,
        dashing_style: None,
        mcid: None,
        tag: None,
    }));
    let ltpage = page.materialize(&arena);
    let mut items = ltpage.iter();
    assert!(matches!(items.next(), Some(LTItem::Line(_))));
    assert!(matches!(items.next(), Some(LTItem::Rect(_))));
    assert!(matches!(items.next(), Some(LTItem::Curve(_))));
}

#[test]
fn test_materialize_image_and_figure() {
    let mut arena = PageArena::new();
    let img_name = arena.intern("Im1");
    let cs = arena.intern("DeviceRGB");
    let fig_name = arena.intern("Fig1");
    let ncolor = arena.intern_color(&[0.0]);
    let scolor = arena.intern_color(&[0.0]);
    let ch = ArenaChar {
        bbox: (1.0, 1.0, 2.0, 2.0),
        text: arena.intern("A"),
        fontname: arena.intern("F1"),
        size: 12.0,
        upright: true,
        adv: 1.0,
        matrix: MATRIX_IDENTITY,
        mcid: None,
        tag: None,
        ncs_name: None,
        scs_name: None,
        ncolor,
        scolor,
    };
    let fig = bolivar_core::arena::types::ArenaFigure {
        name: fig_name,
        bbox: (0.0, 0.0, 10.0, 10.0),
        matrix: MATRIX_IDENTITY,
        items: vec![ArenaItem::Char(ch)],
    };
    let mut page = ArenaPage::new_in(&arena, 1, (0.0, 0.0, 100.0, 100.0));
    page.add(ArenaItem::Image(bolivar_core::arena::types::ArenaImage {
        name: img_name,
        bbox: (0.0, 0.0, 10.0, 10.0),
        srcsize: (Some(10), Some(10)),
        imagemask: false,
        bits: 8,
        colorspace: vec![cs],
    }));
    page.add(ArenaItem::Figure(fig));

    let ltpage = page.materialize(&arena);
    let mut items = ltpage.iter();
    match items.next() {
        Some(LTItem::Image(img)) => {
            assert_eq!(img.name, "Im1");
            assert_eq!(img.srcsize, (Some(10), Some(10)));
            assert_eq!(img.bits, 8);
            assert_eq!(img.colorspace, vec!["DeviceRGB".to_string()]);
        }
        other => panic!("expected image, got {other:?}"),
    }
    match items.next() {
        Some(LTItem::Figure(fig)) => {
            assert_eq!(fig.name, "Fig1");
            assert_eq!(fig.iter().count(), 1);
        }
        other => panic!("expected figure, got {other:?}"),
    }
}
