//! Table extraction (ported from pdfplumber.table)
//!
//! This module provides functionality for extracting tables from PDF pages,
//! including edge detection, cell boundary finding, and text extraction.

mod clustering;
mod collector;
mod edges;
mod finder;
mod geometry;
mod grid;
mod intersections;
pub(crate) mod probe;
mod text;
mod types;

// Re-export public types
pub use types::{
    BBox, CharObj, EdgeObj, ExplicitLine, Orientation, PageGeometry, TableProbePolicy,
    TableSettings, TableStrategy, TextDir, TextSettings, WordObj,
};

pub(crate) use clustering::bbox_overlap;
pub(crate) use collector::collect_table_objects_from_arena;
pub(crate) use edges::{clip_edge_to_bbox, rect_to_edges};
pub(crate) use finder::TableWorkspace;
pub(crate) use finder::{
    extract_metadata_borrowed, extract_spans_borrowed, extract_tables_borrowed,
    extract_tables_with_metadata_from_objects_with_cancellation,
};
pub(crate) use finder::{extract_text_from_objects_borrowed, extract_words_from_objects_borrowed};
pub(crate) use geometry::to_top_left_bbox;

// Re-export public API functions
pub use finder::{
    TableCellMetadata, TableMetadata, TableTextSpans, extract_table_from_ltpage,
    extract_table_from_objects, extract_tables_from_ltpage, extract_tables_from_objects,
    extract_tables_with_metadata_from_objects, extract_text_from_ltpage, extract_text_from_objects,
    extract_words_from_ltpage, extract_words_from_objects,
};

#[cfg(test)]
mod table_extraction_tests {
    use super::geometry::transform_bboxes_batch;
    use super::grid::{build_cells, cells_to_tables, group_cells, intersections_to_cells};
    use super::intersections::{ActiveBucket, IntersectionIdx};
    use super::intersections::{edges_to_intersections, find_intersections};
    use super::text::{extract_text, extract_text_from_char_ids_layout, extract_words};
    use super::types::{
        BBox, BBoxKey, CharId, CharObj, EdgeObj, HEdgeId, KeyPoint, Orientation, TextSettings,
        VEdgeId, bbox_key, key_point,
    };
    use crate::arena::PageArena;
    use crate::cancellation::CancellationToken;
    use crate::error::PdfError;
    use crate::utils::apply_matrix_rect;
    use std::collections::HashMap;

    fn make_v_edge(x: f64, top: f64, bottom: f64) -> EdgeObj {
        EdgeObj {
            x0: x,
            x1: x,
            top,
            bottom,
            width: 0.0,
            height: bottom - top,
            orientation: Some(Orientation::Vertical),
            object_type: "test",
        }
    }

    fn make_h_edge(y: f64, x0: f64, x1: f64) -> EdgeObj {
        EdgeObj {
            x0,
            x1,
            top: y,
            bottom: y,
            width: x1 - x0,
            height: 0.0,
            orientation: Some(Orientation::Horizontal),
            object_type: "test",
        }
    }

    fn edge_key(edge: &EdgeObj) -> BBoxKey {
        bbox_key(&BBox {
            x0: edge.x0,
            top: edge.top,
            x1: edge.x1,
            bottom: edge.bottom,
        })
    }

    fn sample_edges_for_intersections() -> Vec<EdgeObj> {
        vec![
            make_v_edge(0.0, 0.0, 10.0),
            make_v_edge(5.0, 0.0, 10.0),
            make_h_edge(0.0, 0.0, 5.0),
            make_h_edge(5.0, 0.0, 5.0),
            make_h_edge(10.0, 0.0, 5.0),
        ]
    }

    fn scalar_edges_to_intersections(_edges: &[EdgeObj]) -> HashMap<KeyPoint, IntersectionIdx> {
        let mut v_edges: Vec<&EdgeObj> = Vec::new();
        let mut h_edges: Vec<&EdgeObj> = Vec::new();
        for edge in _edges {
            match edge.orientation {
                Some(Orientation::Vertical) => v_edges.push(edge),
                Some(Orientation::Horizontal) => h_edges.push(edge),
                _ => {}
            }
        }

        let mut intersections: HashMap<KeyPoint, IntersectionIdx> = HashMap::new();
        for (v_idx, v) in v_edges.iter().enumerate() {
            for (h_idx, h) in h_edges.iter().enumerate() {
                if v.x0 >= h.x0 && v.x0 <= h.x1 && h.top >= v.top && h.top <= v.bottom {
                    let vertex = key_point(v.x0, h.top);
                    let entry = intersections.entry(vertex).or_insert(IntersectionIdx {
                        v: Vec::new(),
                        h: Vec::new(),
                    });
                    entry.v.push(VEdgeId::from_index(v_idx));
                    entry.h.push(HEdgeId::from_index(h_idx));
                }
            }
        }
        intersections
    }

    fn chars_from_visual_line(arena: &mut PageArena, visual: &str) -> Vec<CharObj> {
        visual
            .chars()
            .enumerate()
            .map(|(i, ch)| {
                let text = arena.intern(&ch.to_string());
                let x = i as f64;
                CharObj {
                    text,
                    x0: x,
                    x1: x + 1.0,
                    top: 0.0,
                    bottom: 10.0,
                    doctop: 0.0,
                    width: 1.0,
                    height: 10.0,
                    size: 10.0,
                    upright: true,
                }
            })
            .collect()
    }

    fn extract_bidi_modes(arena: &mut PageArena, visual: &str) -> (String, String) {
        let chars = chars_from_visual_line(arena, visual);
        let legacy = extract_text(&chars, &TextSettings::default(), arena);
        let bidi = extract_text(
            &chars,
            &TextSettings {
                bidi: true,
                ..TextSettings::default()
            },
            arena,
        );
        (legacy, bidi)
    }

    #[test]
    fn table_extraction_non_consecutive() {
        let edges = vec![
            make_v_edge(0.0, 0.0, 10.0),
            make_v_edge(10.0, 0.0, 10.0),
            make_h_edge(0.0, 0.0, 10.0),
            make_h_edge(5.0, 0.0, 4.0),
            make_h_edge(10.0, 0.0, 10.0),
        ];

        let (store, intersections) = edges_to_intersections(&edges, 0.0, 0.0);
        assert_eq!(intersections.len(), 5);
        for key in [
            key_point(0.0, 0.0),
            key_point(10.0, 0.0),
            key_point(0.0, 5.0),
            key_point(0.0, 10.0),
            key_point(10.0, 10.0),
        ] {
            assert!(intersections.contains_key(&key));
        }

        let cells = intersections_to_cells(&store, &intersections);
        assert_eq!(
            cells,
            vec![BBox {
                x0: 0.0,
                top: 0.0,
                x1: 10.0,
                bottom: 10.0,
            }]
        );
    }

    #[test]
    fn table_extraction_ordering() {
        let edges = vec![
            make_v_edge(0.0, 0.0, 10.0),
            make_v_edge(0.0, 1.0, 9.0),
            make_h_edge(2.0, 0.0, 10.0),
            make_h_edge(2.0, -1.0, 9.0),
        ];

        let (store, intersections) = edges_to_intersections(&edges, 0.0, 0.0);
        let key = key_point(0.0, 2.0);
        let intersection = intersections.get(&key).unwrap();
        let v_keys: Vec<BBoxKey> = intersection
            .v
            .iter()
            .map(|id| edge_key(store.v(*id)))
            .collect();
        let h_keys: Vec<BBoxKey> = intersection
            .h
            .iter()
            .map(|id| edge_key(store.h(*id)))
            .collect();

        let v0 = edge_key(store.v(VEdgeId(0)));
        let v1 = edge_key(store.v(VEdgeId(1)));
        let h0 = edge_key(store.h(HEdgeId(1)));
        let h1 = edge_key(store.h(HEdgeId(0)));
        assert_eq!(v_keys, vec![v0, v0, v1, v1]);
        assert_eq!(h_keys, vec![h1, h0, h1, h0]);
    }

    #[test]
    fn table_extraction_intersection_id_ordering() {
        let edges = vec![
            make_v_edge(0.0, 0.0, 10.0),
            make_v_edge(0.0, 1.0, 9.0),
            make_h_edge(2.0, 0.0, 10.0),
            make_h_edge(2.0, -1.0, 9.0),
        ];

        let (store, intersections) = edges_to_intersections(&edges, 0.0, 0.0);
        let key = key_point(0.0, 2.0);
        let intersection = intersections.get(&key).unwrap();
        assert_eq!(
            intersection.v,
            vec![VEdgeId(0), VEdgeId(0), VEdgeId(1), VEdgeId(1)]
        );
        assert_eq!(
            intersection.h,
            vec![HEdgeId(0), HEdgeId(1), HEdgeId(0), HEdgeId(1)]
        );
        assert_eq!(store.v.len(), 2);
        assert_eq!(store.h.len(), 2);
    }

    #[test]
    fn table_extraction_edge_connects_gap() {
        let edges = vec![
            make_v_edge(0.0, 0.0, 4.0),
            make_v_edge(0.0, 6.0, 10.0),
            make_v_edge(10.0, 0.0, 10.0),
            make_h_edge(2.0, 0.0, 10.0),
            make_h_edge(8.0, 0.0, 10.0),
        ];

        let (store, intersections) = edges_to_intersections(&edges, 0.0, 0.0);
        let cells = intersections_to_cells(&store, &intersections);
        assert!(cells.is_empty());
    }

    #[test]
    fn table_extraction_rowspan_chars() {
        use super::grid::Table;
        let mut arena = PageArena::new();
        arena.reset();
        let text_a = arena.intern("A");
        let text_b = arena.intern("B");

        let table = Table {
            cells: vec![
                BBox {
                    x0: 0.0,
                    top: 0.0,
                    x1: 5.0,
                    bottom: 15.0,
                },
                BBox {
                    x0: 5.0,
                    top: 0.0,
                    x1: 10.0,
                    bottom: 10.0,
                },
                BBox {
                    x0: 5.0,
                    top: 10.0,
                    x1: 10.0,
                    bottom: 20.0,
                },
            ],
        };

        let chars: Vec<CharObj> = vec![
            CharObj {
                text: text_a,
                x0: 1.5,
                x1: 2.5,
                top: 11.5,
                bottom: 12.5,
                doctop: 11.5,
                width: 1.0,
                height: 1.0,
                size: 1.0,
                upright: true,
            },
            CharObj {
                text: text_b,
                x0: 6.5,
                x1: 7.5,
                top: 11.5,
                bottom: 12.5,
                doctop: 11.5,
                width: 1.0,
                height: 1.0,
                size: 1.0,
                upright: true,
            },
        ];

        let settings = TextSettings::default();
        let out = table.extract(&chars, &settings, &arena);
        assert_eq!(
            out,
            vec![
                vec![Some("A".to_string()), Some(String::new())],
                vec![None, Some("B".to_string())],
            ]
        );
    }

    #[test]
    fn table_extraction_two_separate_tables() {
        let edges = vec![
            // Table 1: 2x2 grid at origin
            make_v_edge(0.0, 0.0, 20.0),
            make_v_edge(10.0, 0.0, 20.0),
            make_v_edge(20.0, 0.0, 20.0),
            make_h_edge(0.0, 0.0, 20.0),
            make_h_edge(10.0, 0.0, 20.0),
            make_h_edge(20.0, 0.0, 20.0),
            // Table 2: 2x2 grid offset by 100
            make_v_edge(100.0, 100.0, 120.0),
            make_v_edge(110.0, 100.0, 120.0),
            make_v_edge(120.0, 100.0, 120.0),
            make_h_edge(100.0, 100.0, 120.0),
            make_h_edge(110.0, 100.0, 120.0),
            make_h_edge(120.0, 100.0, 120.0),
        ];

        let (store, intersections) = edges_to_intersections(&edges, 0.0, 0.0);
        let cells = intersections_to_cells(&store, &intersections);
        let tables = cells_to_tables(cells);

        assert_eq!(tables.len(), 2);
        assert_eq!(tables[0].len(), 4);
        assert_eq!(tables[1].len(), 4);
    }

    #[test]
    fn geometry_batch_transform_matches_scalar() {
        let rects = vec![(0.0, 0.0, 1.0, 1.0), (1.0, 1.0, 2.0, 2.0)];
        let m = (1.0, 0.0, 0.0, 1.0, 10.0, 20.0);
        let out = transform_bboxes_batch(m, &rects);
        let scalar: Vec<_> = rects.iter().map(|&r| apply_matrix_rect(m, r)).collect();
        assert_eq!(out, scalar);
    }

    #[test]
    fn table_extraction_text_extraction_basic() {
        let mut arena = PageArena::new();
        arena.reset();
        let text_h = arena.intern("H");
        let text_i = arena.intern("i");
        let chars = vec![
            CharObj {
                text: text_h,
                x0: 0.0,
                x1: 5.0,
                top: 0.0,
                bottom: 10.0,
                doctop: 0.0,
                width: 5.0,
                height: 10.0,
                size: 10.0,
                upright: true,
            },
            CharObj {
                text: text_i,
                x0: 6.0,
                x1: 8.0,
                top: 0.0,
                bottom: 10.0,
                doctop: 0.0,
                width: 2.0,
                height: 10.0,
                size: 10.0,
                upright: true,
            },
        ];

        let settings = TextSettings::default();
        let words = extract_words(&chars, &settings, &arena);

        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "Hi");
    }

    #[test]
    fn table_extraction_text_extraction_reorders_rtl_by_default() {
        let mut arena = PageArena::new();
        arena.reset();
        let text_a = arena.intern("\u{05D0}");
        let text_b = arena.intern("\u{05D1}");
        let text_g = arena.intern("\u{05D2}");
        let chars = vec![
            CharObj {
                text: text_a,
                x0: 0.0,
                x1: 1.0,
                top: 0.0,
                bottom: 10.0,
                doctop: 0.0,
                width: 1.0,
                height: 10.0,
                size: 10.0,
                upright: true,
            },
            CharObj {
                text: text_b,
                x0: 1.0,
                x1: 2.0,
                top: 0.0,
                bottom: 10.0,
                doctop: 0.0,
                width: 1.0,
                height: 10.0,
                size: 10.0,
                upright: true,
            },
            CharObj {
                text: text_g,
                x0: 2.0,
                x1: 3.0,
                top: 0.0,
                bottom: 10.0,
                doctop: 0.0,
                width: 1.0,
                height: 10.0,
                size: 10.0,
                upright: true,
            },
        ];

        let settings = TextSettings::default();
        let text = extract_text(&chars, &settings, &arena);
        assert_eq!(text, "\u{05D2}\u{05D1}\u{05D0}");
    }

    #[test]
    fn table_extraction_layout_text_reorders_rtl_by_default() {
        let mut arena = PageArena::new();
        arena.reset();
        let text_a = arena.intern("\u{05D0}");
        let text_b = arena.intern("\u{05D1}");
        let text_g = arena.intern("\u{05D2}");
        let chars = vec![
            CharObj {
                text: text_a,
                x0: 0.0,
                x1: 1.0,
                top: 0.0,
                bottom: 10.0,
                doctop: 0.0,
                width: 1.0,
                height: 10.0,
                size: 10.0,
                upright: true,
            },
            CharObj {
                text: text_b,
                x0: 1.0,
                x1: 2.0,
                top: 0.0,
                bottom: 10.0,
                doctop: 0.0,
                width: 1.0,
                height: 10.0,
                size: 10.0,
                upright: true,
            },
            CharObj {
                text: text_g,
                x0: 2.0,
                x1: 3.0,
                top: 0.0,
                bottom: 10.0,
                doctop: 0.0,
                width: 1.0,
                height: 10.0,
                size: 10.0,
                upright: true,
            },
        ];
        let ids = vec![CharId(0), CharId(1), CharId(2)];
        let settings = TextSettings::default();
        let layout_bbox = BBox {
            x0: 0.0,
            top: 0.0,
            x1: 0.0,
            bottom: 0.0,
        };

        let text = extract_text_from_char_ids_layout(&chars, &ids, &settings, &layout_bbox, &arena);
        assert_eq!(text, "\u{05D2}\u{05D1}\u{05D0}");
    }

    #[test]
    fn table_extraction_word_extraction_reorders_rtl_by_default() {
        let mut arena = PageArena::new();
        arena.reset();
        let text_a = arena.intern("\u{05D0}");
        let text_b = arena.intern("\u{05D1}");
        let text_g = arena.intern("\u{05D2}");
        let chars = vec![
            CharObj {
                text: text_a,
                x0: 0.0,
                x1: 1.0,
                top: 0.0,
                bottom: 10.0,
                doctop: 0.0,
                width: 1.0,
                height: 10.0,
                size: 10.0,
                upright: true,
            },
            CharObj {
                text: text_b,
                x0: 1.0,
                x1: 2.0,
                top: 0.0,
                bottom: 10.0,
                doctop: 0.0,
                width: 1.0,
                height: 10.0,
                size: 10.0,
                upright: true,
            },
            CharObj {
                text: text_g,
                x0: 2.0,
                x1: 3.0,
                top: 0.0,
                bottom: 10.0,
                doctop: 0.0,
                width: 1.0,
                height: 10.0,
                size: 10.0,
                upright: true,
            },
        ];

        let settings = TextSettings::default();
        let words = extract_words(&chars, &settings, &arena);
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "\u{05D2}\u{05D1}\u{05D0}");
    }

    #[test]
    fn table_extraction_text_reorders_arabic_visual_line_to_logical() {
        let mut arena = PageArena::new();
        arena.reset();

        let visual = "123456 :\u{fe94}\u{fef4}\u{fe91}\u{feae}\u{fecc}\u{fedf}\u{fe8d}";
        let expected = "العربية: 123456";
        let chars = chars_from_visual_line(&mut arena, visual);

        let settings = TextSettings::default();
        let text = extract_text(&chars, &settings, &arena);
        assert_eq!(text, expected);
    }

    #[test]
    fn table_extraction_text_reorders_arabic_visual_words_to_logical() {
        let mut arena = PageArena::new();
        arena.reset();

        let visual = "\u{fe94}\u{fef4}\u{fe91}\u{feae}\u{fecc}\u{fedf}\u{fe8d} \u{fe94}\u{fee0}\u{fee4}\u{fea0}\u{fedf}\u{fe8d}";
        let expected = "الجملة العربية";
        let chars = chars_from_visual_line(&mut arena, visual);

        let settings = TextSettings::default();
        let text = extract_text(&chars, &settings, &arena);
        assert_eq!(text, expected);
    }

    #[test]
    fn table_extraction_text_reorders_hebrew_visual_line_to_logical() {
        let mut arena = PageArena::new();
        arena.reset();

        let visual = "1120280977 :םולש";
        let expected = "שלום: 1120280977";
        let chars = chars_from_visual_line(&mut arena, visual);

        let settings = TextSettings::default();
        let text = extract_text(&chars, &settings, &arena);
        assert_eq!(text, expected);
    }

    #[test]
    fn table_extraction_text_reorders_urdu_visual_line_to_logical() {
        let mut arena = PageArena::new();
        arena.reset();

        let visual = "1120280977 :ہلاوح ربمن";
        let expected = "نمبر حوالہ: 1120280977";
        let chars = chars_from_visual_line(&mut arena, visual);

        let settings = TextSettings::default();
        let text = extract_text(&chars, &settings, &arena);
        assert_eq!(text, expected);
    }

    #[test]
    fn table_extraction_text_preserves_ltr_run_order_in_hebrew_mixed_line() {
        let mut arena = PageArena::new();
        arena.reset();

        let visual = "SYDALYT ALCETRINE - םולש";
        let expected = "שלום - SYDALYT ALCETRINE";
        let chars = chars_from_visual_line(&mut arena, visual);

        let settings = TextSettings::default();
        let text = extract_text(&chars, &settings, &arena);
        assert_eq!(text, expected);
    }

    #[test]
    fn table_extraction_text_preserves_ltr_run_order_in_urdu_mixed_line() {
        let mut arena = PageArena::new();
        arena.reset();

        let visual = "SYDALYT ALCETRINE - ہیفرم ربمن";
        let expected = "نمبر مرفیہ - SYDALYT ALCETRINE";
        let chars = chars_from_visual_line(&mut arena, visual);

        let settings = TextSettings::default();
        let text = extract_text(&chars, &settings, &arena);
        assert_eq!(text, expected);
    }

    #[test]
    fn table_extraction_reconstructs_compact_mixed_fields() {
        let mut arena = PageArena::new();
        arena.reset();

        let visual = "Task Ref42 12:34:\u{fe94}\u{fec8}\u{fea3}\u{fefc}\u{fee3}**56:78:\u{fe96}\u{fed7}\u{feee}\u{fedf}\u{fe8d}";
        let expected = "Task Ref42 12:34:الوقت:56:78**ملاحظة";
        let (_, bidi) = extract_bidi_modes(&mut arena, visual);

        assert_eq!(bidi, expected);
    }

    #[test]
    fn table_bidi_keeps_ambiguous_bilingual_order() {
        let mut arena = PageArena::new();
        arena.reset();

        let (legacy, bidi) = extract_bidi_modes(
            &mut arena,
            "English \u{fe94}\u{fef4}\u{fe91}\u{feae}\u{fecc}\u{fedf}\u{fe8d}",
        );

        assert_eq!(legacy, "العربية English");
        assert_eq!(bidi, legacy);
    }

    #[test]
    fn table_bidi_keeps_separate_numeric_words_in_place() {
        let mut arena = PageArena::new();
        arena.reset();

        let (legacy, bidi) = extract_bidi_modes(
            &mut arena,
            "100.25 42.00 \u{fe94}\u{fef4}\u{fe91}\u{feae}\u{fecc}\u{fedf}\u{fe8d} .1 : \u{fe94}\u{fee0}\u{fee4}\u{fea0}\u{fedf}\u{fe8d} .TXT : \u{feba}\u{fee8}\u{fedf}\u{fe8d} .CODE",
        );

        assert_eq!(bidi, legacy);
    }

    #[test]
    fn table_bidi_keeps_rtl_prefixed_compact_run_in_place() {
        let mut arena = PageArena::new();
        arena.reset();

        let (legacy, bidi) =
            extract_bidi_modes(&mut arena, "Alpha #Beta 10:41 في 2024-11-01 علىPM");

        assert_eq!(bidi, legacy);
    }

    #[test]
    fn intersections_swap_pop_updates_slot() {
        let mut active = vec![ActiveBucket::default()];
        let bucket_idx = 0usize;
        let mut bucket = ActiveBucket::default();
        let slot0 = bucket.insert(0, &make_v_edge(1.0, 0.0, 10.0));
        let slot1 = bucket.insert(1, &make_v_edge(1.0, 0.0, 10.0));
        let slot2 = bucket.insert(2, &make_v_edge(1.0, 0.0, 10.0));
        active[bucket_idx] = bucket;

        let mut active_slots = vec![None; 3];
        active_slots[0] = Some((bucket_idx, slot0));
        active_slots[1] = Some((bucket_idx, slot1));
        active_slots[2] = Some((bucket_idx, slot2));

        super::intersections::remove_active_entry(&mut active, &mut active_slots, 1);

        let bucket = active.get(bucket_idx).unwrap();
        assert_eq!(bucket.active_len(), 2);
        assert!(active_slots[1].is_none());
        assert!(active_slots[0].is_some());
        assert!(active_slots[2].is_some());
    }

    #[test]
    fn intersections_swap_pop_removes_empty_bucket() {
        let mut active = vec![ActiveBucket::default()];
        let bucket_idx = 0usize;
        let mut bucket = ActiveBucket::default();
        let slot0 = bucket.insert(0, &make_v_edge(2.0, 0.0, 10.0));
        active[bucket_idx] = bucket;

        let mut active_slots = vec![None; 1];
        active_slots[0] = Some((bucket_idx, slot0));

        super::intersections::remove_active_entry(&mut active, &mut active_slots, 0);

        assert_eq!(active[bucket_idx].active_len(), 0);
        assert!(active_slots[0].is_none());
    }

    #[test]
    fn intersections_aosoa_produces_same_count() {
        let edges = sample_edges_for_intersections();
        let (_store, out) = edges_to_intersections(&edges, 0.0, 0.0);
        assert_eq!(out.len(), scalar_edges_to_intersections(&edges).len());
    }

    #[test]
    fn intersections_bucketed_matches_scalar_count() {
        let edges = sample_edges_for_intersections();
        let (_store, out) = edges_to_intersections(&edges, 0.0, 0.0);
        assert_eq!(out.len(), scalar_edges_to_intersections(&edges).len());
        let buckets = super::intersections::bucket_count_for_edges(&edges);
        assert!(buckets > 0);
    }

    #[test]
    fn intersection_buckets_ignore_coordinate_span() {
        let edges = vec![
            make_v_edge(0.0, 0.0, 10.0),
            make_v_edge(1_000_000_000.0, 0.0, 10.0),
        ];

        let buckets = super::intersections::bucket_count_for_edges(&edges);

        assert!(buckets <= edges.len());
    }

    #[test]
    fn intersections_support_distant_edges() {
        let edges = vec![
            make_v_edge(0.0, 0.0, 10.0),
            make_v_edge(1_000_000_000.0, 0.0, 10.0),
            make_h_edge(0.0, 0.0, 1_000_000_000.0),
            make_h_edge(10.0, 0.0, 1_000_000_000.0),
        ];

        let (store, intersections) = edges_to_intersections(&edges, 3.0, 3.0);
        let cells = intersections_to_cells(&store, &intersections);

        assert_eq!(intersections.len(), 4);
        assert_eq!(cells.len(), 1);
    }

    #[test]
    fn table_kernels_honor_cancellation() {
        let edges = sample_edges_for_intersections();
        let live = CancellationToken::new();
        let (store, intersections) = find_intersections(&edges, 0.0, 0.0, &live).unwrap();
        let cells = build_cells(&store, &intersections, &live).unwrap();
        let cancelled = CancellationToken::new();
        cancelled.cancel();

        assert!(matches!(
            find_intersections(&edges, 0.0, 0.0, &cancelled),
            Err(PdfError::Cancelled)
        ));
        assert!(matches!(
            build_cells(&store, &intersections, &cancelled),
            Err(PdfError::Cancelled)
        ));
        assert!(matches!(
            group_cells(cells, &cancelled),
            Err(PdfError::Cancelled)
        ));
    }

    #[test]
    fn cell_matching_soa_matches_scalar() {
        let mut arena = PageArena::new();
        arena.reset();
        let text_a = arena.intern("A");
        let text_b = arena.intern("B");
        let cells = vec![
            BBox {
                x0: 0.0,
                top: 0.0,
                x1: 5.0,
                bottom: 5.0,
            },
            BBox {
                x0: 5.0,
                top: 0.0,
                x1: 10.0,
                bottom: 5.0,
            },
        ];
        let chars = vec![
            CharObj {
                text: text_a,
                x0: 1.0,
                x1: 2.0,
                top: 1.0,
                bottom: 2.0,
                doctop: 1.0,
                width: 1.0,
                height: 1.0,
                size: 1.0,
                upright: true,
            },
            CharObj {
                text: text_b,
                x0: 6.0,
                x1: 7.0,
                top: 1.0,
                bottom: 2.0,
                doctop: 1.0,
                width: 1.0,
                height: 1.0,
                size: 1.0,
                upright: true,
            },
        ];
        let out_a = super::grid::Table {
            cells: cells.clone(),
        }
        .extract(&chars, &TextSettings::default(), &arena);
        let out_b =
            super::grid::Table { cells }.extract_soa(&chars, &TextSettings::default(), &arena);
        assert_eq!(out_a, out_b);
    }

    #[test]
    fn charobj_is_compact() {
        assert!(
            std::mem::size_of::<CharObj>() <= 88,
            "CharObj too large: {} bytes",
            std::mem::size_of::<CharObj>()
        );
    }
}
