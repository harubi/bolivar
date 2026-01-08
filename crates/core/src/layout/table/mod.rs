//! Table extraction (ported from pdfplumber.table)
//!
//! This module provides functionality for extracting tables from PDF pages,
//! including edge detection, cell boundary finding, and text extraction.

mod clustering;
mod edges;
mod finder;
mod geometry;
mod grid;
mod intersections;
mod text;
mod types;

// Re-export public types
pub use types::{
    BBox, CharObj, EdgeObj, ExplicitLine, Orientation, PageGeometry, TableSettings, TextDir,
    TextSettings, WordObj,
};

// Re-export public API functions
pub use finder::{
    extract_table_from_ltpage, extract_table_from_objects, extract_tables_from_ltpage,
    extract_tables_from_objects, extract_text_from_ltpage, extract_words_from_ltpage,
};

#[cfg(test)]
mod table_extraction_tests {
    use super::grid::{cells_to_tables, intersections_to_cells};
    use super::intersections::edges_to_intersections;
    use super::text::extract_words;
    use super::types::{
        BBox, BBoxKey, CharObj, EdgeObj, HEdgeId, Orientation, TextSettings, VEdgeId, bbox_key,
        key_point,
    };

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
                text: "A".to_string(),
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
                text: "B".to_string(),
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
        let out = table.extract(&chars, &settings);
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
    fn table_extraction_text_extraction_basic() {
        let chars = vec![
            CharObj {
                text: "H".to_string(),
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
                text: "i".to_string(),
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
        let words = extract_words(&chars, &settings);

        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "Hi");
    }

    #[test]
    fn intersections_simd_mask_expected() {
        let tops = [-5.0, -1.0, 1.0, -5.0];
        let bottoms = [5.0, 1.0, 5.0, 5.0];
        let x0s = [5.0, 7.0, 2.0, 20.0];
        let mask =
            super::intersections::match_v_edges_simd4(tops, bottoms, x0s, 0.0, 0.0, 10.0, 0.0);
        assert_eq!(mask, [true, true, false, false]);
    }

    #[test]
    fn char_in_bboxes_simd4_expected() {
        let h_mid = 5.0;
        let v_mid = 5.0;
        let x0s = [0.0, 6.0, 0.0, 4.0];
        let x1s = [10.0, 8.0, 10.0, 6.0];
        let tops = [0.0, 0.0, 6.0, 4.0];
        let bottoms = [10.0, 10.0, 8.0, 6.0];
        let mask = super::grid::char_in_bboxes_simd4(h_mid, v_mid, x0s, x1s, tops, bottoms);
        assert_eq!(mask, [true, false, false, true]);
    }
}
