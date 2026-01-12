//! TableFinder orchestrator and public API for table extraction.
//!
//! This module provides the main entry points for extracting tables,
//! words, and text from PDF pages.

use crate::arena::{ArenaLookup, PageArena};
use crate::layout::types::LTChar;
use crate::utils::{HasBBox, Rect};

use super::clustering::{bbox_overlap, bbox_overlap_strict};
use super::edges::{
    clip_edge_to_bbox, curve_to_edges, filter_edges, merge_edges, rect_to_edges, words_to_edges_h,
    words_to_edges_v,
};
use super::geometry::{to_top_left_bbox, to_top_left_bboxes_batch};
use super::grid::{Table, cells_to_tables, intersections_to_cells};
use super::intersections::edges_to_intersections;
use super::text::{extract_text, extract_words};
use super::types::{
    BBox, CharObj, EdgeObj, ExplicitLine, Orientation, PageGeometry, TableSettings, TextSettings,
    WordObj,
};
use crate::layout::types::{LTItem, LTPage, TextBoxType, TextLineElement, TextLineType};

/// Check if two rectangles are equal within epsilon.
fn rects_equal(a: Rect, b: Rect) -> bool {
    const EPS: f64 = 1e-6;
    (a.0 - b.0).abs() < EPS
        && (a.1 - b.1).abs() < EPS
        && (a.2 - b.2).abs() < EPS
        && (a.3 - b.3).abs() < EPS
}

/// Convert an LTChar to CharObj, applying crop and coordinate transform.
fn char_to_charobj(
    c: &LTChar,
    geom: &PageGeometry,
    crop_bbox: Option<BBox>,
    arena: &mut PageArena,
) -> Option<CharObj> {
    let bbox = to_top_left_bbox(c.x0(), c.y0(), c.x1(), c.y1(), geom);
    let bbox = if let Some(crop) = crop_bbox {
        bbox_overlap(bbox, crop)?
    } else {
        bbox
    };
    let text = arena.intern(c.get_text());
    Some(CharObj {
        text,
        x0: bbox.x0,
        x1: bbox.x1,
        top: bbox.top,
        bottom: bbox.bottom,
        doctop: geom.initial_doctop + bbox.top,
        width: bbox.width(),
        height: bbox.height(),
        size: c.size(),
        upright: c.upright(),
    })
}

/// Collect all characters and edges from a page.
fn collect_page_objects(
    page: &LTPage,
    geom: &PageGeometry,
    arena: &mut PageArena,
) -> (Vec<CharObj>, Vec<EdgeObj>) {
    let mut chars: Vec<CharObj> = Vec::new();
    let mut edges: Vec<EdgeObj> = Vec::new();
    let mut rects: Vec<Rect> = Vec::new();

    fn visit_item(
        item: &LTItem,
        geom: &PageGeometry,
        crop_bbox: Option<BBox>,
        arena: &mut PageArena,
        chars: &mut Vec<CharObj>,
        edges: &mut Vec<EdgeObj>,
        rects: &mut Vec<Rect>,
    ) {
        match item {
            LTItem::Char(c) => {
                if let Some(obj) = char_to_charobj(c, geom, crop_bbox, arena) {
                    chars.push(obj);
                }
            }
            LTItem::Line(l) => {
                let bbox = to_top_left_bbox(l.x0(), l.y0(), l.x1(), l.y1(), geom);
                let edge = EdgeObj {
                    x0: bbox.x0,
                    x1: bbox.x1,
                    top: bbox.top,
                    bottom: bbox.bottom,
                    width: bbox.width(),
                    height: bbox.height(),
                    orientation: if bbox.top == bbox.bottom {
                        Some(Orientation::Horizontal)
                    } else {
                        Some(Orientation::Vertical)
                    },
                    object_type: "line",
                };
                if let Some(crop) = crop_bbox {
                    if let Some(edge) = clip_edge_to_bbox(edge, crop) {
                        edges.push(edge);
                    }
                } else {
                    edges.push(edge);
                }
            }
            LTItem::Rect(r) => {
                rects.push(r.bbox());
            }
            LTItem::Curve(c) => {
                let mut pts = Vec::new();
                for p in &c.pts {
                    let tl = to_top_left_bbox(p.0, p.1, p.0, p.1, geom);
                    pts.push((tl.x0, tl.top));
                }
                for edge in curve_to_edges(&pts, "curve_edge") {
                    if let Some(crop) = crop_bbox {
                        if let Some(edge) = clip_edge_to_bbox(edge, crop) {
                            edges.push(edge);
                        }
                    } else {
                        edges.push(edge);
                    }
                }
            }
            LTItem::TextLine(line) => {
                let mut push_chars = |elements: &mut dyn Iterator<Item = &TextLineElement>| {
                    for el in elements {
                        if let TextLineElement::Char(c) = el {
                            if let Some(obj) = char_to_charobj(c, geom, crop_bbox, arena) {
                                chars.push(obj);
                            }
                        }
                    }
                };
                match line {
                    TextLineType::Horizontal(l) => push_chars(&mut l.iter()),
                    TextLineType::Vertical(l) => push_chars(&mut l.iter()),
                }
            }
            LTItem::TextBox(tb) => {
                let mut push_line_chars = |elements: &mut dyn Iterator<Item = &TextLineElement>| {
                    for el in elements {
                        if let TextLineElement::Char(c) = el {
                            if let Some(obj) = char_to_charobj(c, geom, crop_bbox, arena) {
                                chars.push(obj);
                            }
                        }
                    }
                };
                match tb {
                    TextBoxType::Horizontal(b) => {
                        for line in b.iter() {
                            push_line_chars(&mut line.iter());
                        }
                    }
                    TextBoxType::Vertical(b) => {
                        for line in b.iter() {
                            push_line_chars(&mut line.iter());
                        }
                    }
                }
            }
            LTItem::Figure(fig) => {
                for child in fig.iter() {
                    visit_item(child, geom, crop_bbox, arena, chars, edges, rects);
                }
            }
            LTItem::Page(p) => {
                for child in p.iter() {
                    visit_item(child, geom, crop_bbox, arena, chars, edges, rects);
                }
            }
            _ => {}
        }
    }

    let crop_bbox = if rects_equal(geom.page_bbox, geom.mediabox) {
        None
    } else {
        Some(BBox {
            x0: geom.page_bbox.0,
            top: geom.page_bbox.1,
            x1: geom.page_bbox.2,
            bottom: geom.page_bbox.3,
        })
    };

    for item in page.iter() {
        visit_item(
            item, geom, crop_bbox, arena, &mut chars, &mut edges, &mut rects,
        );
    }

    if !rects.is_empty() {
        let bboxes = to_top_left_bboxes_batch(&rects, geom);
        for bbox in bboxes {
            for edge in rect_to_edges(bbox) {
                if let Some(crop) = crop_bbox {
                    if let Some(edge) = clip_edge_to_bbox(edge, crop) {
                        edges.push(edge);
                    }
                } else {
                    edges.push(edge);
                }
            }
        }
    }

    (chars, edges)
}

/// Main table finder that orchestrates the extraction pipeline.
struct TableFinder<'a> {
    page_bbox: BBox,
    chars: Vec<CharObj>,
    edges: Vec<EdgeObj>,
    settings: TableSettings,
    arena: &'a dyn ArenaLookup,
}

impl<'a> TableFinder<'a> {
    fn new(
        page: &LTPage,
        geom: &PageGeometry,
        settings: TableSettings,
        arena: &'a mut PageArena,
    ) -> Self {
        let (chars, edges) = collect_page_objects(page, geom, arena);
        let arena_lookup: &dyn ArenaLookup = arena;
        let page_bbox = BBox {
            x0: geom.page_bbox.0,
            top: geom.page_bbox.1,
            x1: geom.page_bbox.2,
            bottom: geom.page_bbox.3,
        };
        Self {
            page_bbox,
            chars,
            edges,
            settings,
            arena: arena_lookup,
        }
    }

    fn from_objects(
        chars: Vec<CharObj>,
        edges: Vec<EdgeObj>,
        geom: &PageGeometry,
        settings: TableSettings,
        arena: &'a dyn ArenaLookup,
    ) -> Self {
        let page_bbox = BBox {
            x0: geom.page_bbox.0,
            top: geom.page_bbox.1,
            x1: geom.page_bbox.2,
            bottom: geom.page_bbox.3,
        };
        Self {
            page_bbox,
            chars,
            edges,
            settings,
            arena,
        }
    }

    fn get_edges(&self) -> Vec<EdgeObj> {
        let settings = &self.settings;

        let v_strat = settings.vertical_strategy.as_str();
        let h_strat = settings.horizontal_strategy.as_str();

        let mut words: Vec<WordObj> = Vec::new();
        if v_strat == "text" || h_strat == "text" {
            words = extract_words(&self.chars, &settings.text_settings, self.arena);
        }

        // explicit vertical lines
        let mut v_explicit: Vec<EdgeObj> = Vec::new();
        for desc in &settings.explicit_vertical_lines {
            match desc {
                ExplicitLine::Coord(x) => v_explicit.push(EdgeObj {
                    x0: *x,
                    x1: *x,
                    top: self.page_bbox.top,
                    bottom: self.page_bbox.bottom,
                    width: 0.0,
                    height: self.page_bbox.bottom - self.page_bbox.top,
                    orientation: Some(Orientation::Vertical),
                    object_type: "explicit_edge",
                }),
                ExplicitLine::Edge(e) => {
                    if e.orientation == Some(Orientation::Vertical) {
                        v_explicit.push(e.clone())
                    }
                }
                ExplicitLine::Rect(b) => {
                    v_explicit.extend(
                        rect_to_edges(*b)
                            .into_iter()
                            .filter(|e| e.orientation == Some(Orientation::Vertical)),
                    );
                }
                ExplicitLine::Curve(pts) => {
                    v_explicit.extend(
                        curve_to_edges(pts, "curve_edge")
                            .into_iter()
                            .filter(|e| e.orientation == Some(Orientation::Vertical)),
                    );
                }
            }
        }

        let mut v_base = Vec::new();
        if v_strat == "lines" {
            v_base = filter_edges(
                self.edges.clone(),
                Some(Orientation::Vertical),
                None,
                settings.edge_min_length_prefilter,
            );
        } else if v_strat == "lines_strict" {
            v_base = filter_edges(
                self.edges.clone(),
                Some(Orientation::Vertical),
                Some("line"),
                settings.edge_min_length_prefilter,
            );
        } else if v_strat == "text" {
            v_base = words_to_edges_v(&words, settings.min_words_vertical);
        }

        let mut v = v_base;
        v.extend(v_explicit);

        // explicit horizontal lines
        let mut h_explicit: Vec<EdgeObj> = Vec::new();
        for desc in &settings.explicit_horizontal_lines {
            match desc {
                ExplicitLine::Coord(y) => h_explicit.push(EdgeObj {
                    x0: self.page_bbox.x0,
                    x1: self.page_bbox.x1,
                    top: *y,
                    bottom: *y,
                    width: self.page_bbox.x1 - self.page_bbox.x0,
                    height: 0.0,
                    orientation: Some(Orientation::Horizontal),
                    object_type: "explicit_edge",
                }),
                ExplicitLine::Edge(e) => {
                    if e.orientation == Some(Orientation::Horizontal) {
                        h_explicit.push(e.clone())
                    }
                }
                ExplicitLine::Rect(b) => {
                    h_explicit.extend(
                        rect_to_edges(*b)
                            .into_iter()
                            .filter(|e| e.orientation == Some(Orientation::Horizontal)),
                    );
                }
                ExplicitLine::Curve(pts) => {
                    h_explicit.extend(
                        curve_to_edges(pts, "curve_edge")
                            .into_iter()
                            .filter(|e| e.orientation == Some(Orientation::Horizontal)),
                    );
                }
            }
        }

        let mut h_base = Vec::new();
        if h_strat == "lines" {
            h_base = filter_edges(
                self.edges.clone(),
                Some(Orientation::Horizontal),
                None,
                settings.edge_min_length_prefilter,
            );
        } else if h_strat == "lines_strict" {
            h_base = filter_edges(
                self.edges.clone(),
                Some(Orientation::Horizontal),
                Some("line"),
                settings.edge_min_length_prefilter,
            );
        } else if h_strat == "text" {
            h_base = words_to_edges_h(&words, settings.min_words_horizontal);
        }

        let mut h = h_base;
        h.extend(h_explicit);

        let mut edges = v;
        edges.extend(h);

        let edges = merge_edges(
            edges,
            settings.snap_x_tolerance,
            settings.snap_y_tolerance,
            settings.join_x_tolerance,
            settings.join_y_tolerance,
        );

        filter_edges(edges, None, None, settings.edge_min_length)
    }

    fn find_tables(&self) -> Vec<Table> {
        let edges = self.get_edges();
        let (store, intersections) = edges_to_intersections(
            &edges,
            self.settings.intersection_x_tolerance,
            self.settings.intersection_y_tolerance,
        );
        let cells = intersections_to_cells(&store, &intersections);
        let tables = cells_to_tables(cells);
        tables
            .into_iter()
            .map(|cell_group| Table { cells: cell_group })
            .collect()
    }
}

/// Extract all tables from a page as nested vectors of cell text.
pub fn extract_tables_from_ltpage(
    page: &LTPage,
    geom: &PageGeometry,
    settings: &TableSettings,
) -> Vec<Vec<Vec<Option<String>>>> {
    let mut arena = PageArena::new();
    arena.reset();
    let finder = TableFinder::new(page, geom, settings.clone(), &mut arena);
    let mut tables = finder.find_tables();
    if geom.force_crop {
        let crop = BBox {
            x0: geom.page_bbox.0,
            top: geom.page_bbox.1,
            x1: geom.page_bbox.2,
            bottom: geom.page_bbox.3,
        };
        tables.retain(|t| bbox_overlap_strict(t.bbox(), crop));
    }
    tables
        .iter()
        .map(|t| t.extract(&finder.chars, &settings.text_settings, finder.arena))
        .collect()
}

/// Extract all tables from precomputed characters/edges.
pub fn extract_tables_from_objects(
    chars: Vec<CharObj>,
    edges: Vec<EdgeObj>,
    geom: &PageGeometry,
    settings: &TableSettings,
    arena: &impl ArenaLookup,
) -> Vec<Vec<Vec<Option<String>>>> {
    let arena: &dyn ArenaLookup = arena;
    let finder = TableFinder::from_objects(chars, edges, geom, settings.clone(), arena);
    let mut tables = finder.find_tables();
    if geom.force_crop {
        let crop = BBox {
            x0: geom.page_bbox.0,
            top: geom.page_bbox.1,
            x1: geom.page_bbox.2,
            bottom: geom.page_bbox.3,
        };
        tables.retain(|t| bbox_overlap_strict(t.bbox(), crop));
    }
    tables
        .iter()
        .map(|t| t.extract(&finder.chars, &settings.text_settings, finder.arena))
        .collect()
}

/// Extract the largest table from a page.
pub fn extract_table_from_ltpage(
    page: &LTPage,
    geom: &PageGeometry,
    settings: &TableSettings,
) -> Option<Vec<Vec<Option<String>>>> {
    let mut arena = PageArena::new();
    arena.reset();
    let finder = TableFinder::new(page, geom, settings.clone(), &mut arena);
    let mut tables = finder.find_tables();
    if geom.force_crop {
        let crop = BBox {
            x0: geom.page_bbox.0,
            top: geom.page_bbox.1,
            x1: geom.page_bbox.2,
            bottom: geom.page_bbox.3,
        };
        tables.retain(|t| bbox_overlap_strict(t.bbox(), crop));
    }
    if tables.is_empty() {
        return None;
    }

    let mut best_idx = 0usize;
    for (idx, table) in tables.iter().enumerate().skip(1) {
        let best = &tables[best_idx];
        let table_cells = table.cells.len();
        let best_cells = best.cells.len();
        if table_cells > best_cells {
            best_idx = idx;
            continue;
        }
        if table_cells == best_cells {
            let table_bbox = table.bbox();
            let best_bbox = best.bbox();
            let top_cmp = table_bbox
                .top
                .partial_cmp(&best_bbox.top)
                .unwrap_or(std::cmp::Ordering::Equal);
            if top_cmp == std::cmp::Ordering::Less {
                best_idx = idx;
                continue;
            }
            if top_cmp == std::cmp::Ordering::Equal {
                let x_cmp = table_bbox
                    .x0
                    .partial_cmp(&best_bbox.x0)
                    .unwrap_or(std::cmp::Ordering::Equal);
                if x_cmp == std::cmp::Ordering::Less {
                    best_idx = idx;
                }
            }
        }
    }

    Some(tables[best_idx].extract(&finder.chars, &settings.text_settings, finder.arena))
}

/// Extract the largest table from precomputed characters/edges.
pub fn extract_table_from_objects(
    chars: Vec<CharObj>,
    edges: Vec<EdgeObj>,
    geom: &PageGeometry,
    settings: &TableSettings,
    arena: &impl ArenaLookup,
) -> Option<Vec<Vec<Option<String>>>> {
    let arena: &dyn ArenaLookup = arena;
    let finder = TableFinder::from_objects(chars, edges, geom, settings.clone(), arena);
    let mut tables = finder.find_tables();
    if geom.force_crop {
        let crop = BBox {
            x0: geom.page_bbox.0,
            top: geom.page_bbox.1,
            x1: geom.page_bbox.2,
            bottom: geom.page_bbox.3,
        };
        tables.retain(|t| bbox_overlap_strict(t.bbox(), crop));
    }
    if tables.is_empty() {
        return None;
    }

    let mut best_idx = 0usize;
    for (idx, table) in tables.iter().enumerate().skip(1) {
        let best = &tables[best_idx];
        let table_cells = table.cells.len();
        let best_cells = best.cells.len();
        if table_cells > best_cells {
            best_idx = idx;
            continue;
        }
        if table_cells == best_cells {
            let table_bbox = table.bbox();
            let best_bbox = best.bbox();
            let top_cmp = table_bbox
                .top
                .partial_cmp(&best_bbox.top)
                .unwrap_or(std::cmp::Ordering::Equal);
            if top_cmp == std::cmp::Ordering::Less {
                best_idx = idx;
                continue;
            }
            if top_cmp == std::cmp::Ordering::Equal {
                let x_cmp = table_bbox
                    .x0
                    .partial_cmp(&best_bbox.x0)
                    .unwrap_or(std::cmp::Ordering::Equal);
                if x_cmp == std::cmp::Ordering::Less {
                    best_idx = idx;
                }
            }
        }
    }

    Some(tables[best_idx].extract(&finder.chars, &settings.text_settings, finder.arena))
}

/// Extract words from a page.
pub fn extract_words_from_ltpage(
    page: &LTPage,
    geom: &PageGeometry,
    settings: TextSettings,
) -> Vec<WordObj> {
    let mut arena = PageArena::new();
    arena.reset();
    let (chars, _edges) = collect_page_objects(page, geom, &mut arena);
    extract_words(&chars, &settings, &arena)
}

/// Extract text from a page.
pub fn extract_text_from_ltpage(
    page: &LTPage,
    geom: &PageGeometry,
    settings: TextSettings,
) -> String {
    let mut arena = PageArena::new();
    arena.reset();
    let (chars, _edges) = collect_page_objects(page, geom, &mut arena);
    extract_text(&chars, &settings, &arena)
}

#[cfg(test)]
mod tests {
    use super::collect_page_objects;
    use crate::arena::PageArena;
    use crate::arena::types::{ArenaChar, ArenaItem, ArenaLine, ArenaPage, ArenaRect};
    use crate::layout::table::collect_table_objects_from_arena;
    use crate::layout::table::types::PageGeometry;
    use crate::utils::Rect;

    #[test]
    fn collect_table_objects_from_arena_matches_ltpage() {
        let mut arena = PageArena::new();
        let mut ctx = arena.context();
        let bbox: Rect = (0.0, 0.0, 100.0, 100.0);
        let color = ctx.intern_color(&[0.0, 0.0, 0.0]);
        let text = ctx.intern("A");
        let font = ctx.intern("F");
        let mut page = ArenaPage::new_in(&ctx, 1, bbox);

        let ch = ArenaChar {
            bbox: (10.0, 20.0, 12.0, 30.0),
            text,
            fontname: font,
            size: 10.0,
            upright: true,
            adv: 2.0,
            matrix: (1.0, 0.0, 0.0, 1.0, 0.0, 0.0),
            mcid: None,
            tag: None,
            ncs_name: None,
            scs_name: None,
            ncolor: color,
            scolor: color,
        };
        page.add(ArenaItem::Char(ch));

        let line = ArenaLine {
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
        };
        page.add(ArenaItem::Line(line));

        let rect = ArenaRect {
            linewidth: 1.0,
            bbox: (5.0, 5.0, 15.0, 15.0),
            stroke: true,
            fill: false,
            evenodd: false,
            stroking_color: color,
            non_stroking_color: color,
            original_path: None,
            dashing_style: None,
            mcid: None,
            tag: None,
        };
        page.add(ArenaItem::Rect(rect));

        let geom = PageGeometry {
            page_bbox: bbox,
            mediabox: bbox,
            initial_doctop: 0.0,
            force_crop: false,
        };

        let ltpage = page.clone().materialize(&ctx);
        let mut lt_arena = PageArena::new();
        lt_arena.reset();
        let (chars_lt, edges_lt) = collect_page_objects(&ltpage, &geom, &mut lt_arena);
        let (chars_arena, edges_arena) = collect_table_objects_from_arena(&page, &geom);

        assert_eq!(chars_lt.len(), chars_arena.len());
        assert_eq!(edges_lt.len(), edges_arena.len());
        assert_eq!(
            lt_arena.resolve(chars_lt[0].text),
            ctx.resolve(chars_arena[0].text)
        );
    }
}
