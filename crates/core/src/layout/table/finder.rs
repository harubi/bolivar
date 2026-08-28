//! TableFinder orchestrator and public API for table extraction.
//!
//! This module provides the main entry points for extracting tables,
//! words, and text from PDF pages.

use std::borrow::Cow;

use crate::arena::{ArenaLookup, PageArena};
use crate::cancellation::CancellationToken;
use crate::error::Result;
use crate::layout::types::LTChar;
use crate::utils::{HasBBox, Rect};

use super::clustering::{bbox_overlap, bbox_overlap_strict};
use super::edges::{
    clip_edge_to_bbox, curve_to_edges, merge_edges_cancellable, rect_to_edges,
    words_to_edges_h_cancellable, words_to_edges_v_cancellable,
};
use super::geometry::{to_top_left_bbox, to_top_left_bboxes_batch};
use super::grid::{Table, build_cells, group_cells};
use super::intersections::find_intersections;
use super::text::TextSpan;
use super::text::{extract_text, extract_words, extract_words_cancellable};
use super::types::{
    BBox, CharObj, EdgeObj, ExplicitLine, Orientation, PageGeometry, TableSettings, TableStrategy,
    TextSettings, WordObj,
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
                        if let TextLineElement::Char(c) = el
                            && let Some(obj) = char_to_charobj(c, geom, crop_bbox, arena)
                        {
                            chars.push(obj);
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
                        if let TextLineElement::Char(c) = el
                            && let Some(obj) = char_to_charobj(c, geom, crop_bbox, arena)
                        {
                            chars.push(obj);
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
    chars: Cow<'a, [CharObj]>,
    edges: Cow<'a, [EdgeObj]>,
    settings: &'a TableSettings,
    arena: &'a dyn ArenaLookup,
}

#[derive(Default)]
pub(crate) struct TableWorkspace {
    edges: Vec<EdgeObj>,
}

impl<'a> TableFinder<'a> {
    fn new(
        page: &LTPage,
        geom: &PageGeometry,
        settings: &'a TableSettings,
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
            chars: Cow::Owned(chars),
            edges: Cow::Owned(edges),
            settings,
            arena: arena_lookup,
        }
    }

    fn from_objects(
        chars: &'a [CharObj],
        edges: &'a [EdgeObj],
        geom: &PageGeometry,
        settings: &'a TableSettings,
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
            chars: Cow::Borrowed(chars),
            edges: Cow::Borrowed(edges),
            settings,
            arena,
        }
    }

    #[hotpath::measure]
    fn get_edges_with_cancellation(
        &self,
        cancellation: &CancellationToken,
        edges: &mut Vec<EdgeObj>,
    ) -> Result<()> {
        cancellation.check()?;
        let settings = &self.settings;
        edges.clear();

        let v_strat = settings.vertical_strategy;
        let h_strat = settings.horizontal_strategy;

        let mut words: Vec<WordObj> = Vec::new();
        if v_strat.uses_text() || h_strat.uses_text() {
            words = extract_words_cancellable(
                &self.chars,
                &settings.text_settings,
                self.arena,
                cancellation,
            )?;
        }

        match v_strat {
            TableStrategy::Lines => {
                edges.extend(
                    self.edges
                        .iter()
                        .filter(|edge| {
                            edge.orientation == Some(Orientation::Vertical)
                                && edge.height >= settings.edge_min_length_prefilter
                        })
                        .cloned(),
                );
            }
            TableStrategy::LinesStrict => {
                edges.extend(
                    self.edges
                        .iter()
                        .filter(|edge| {
                            edge.orientation == Some(Orientation::Vertical)
                                && edge.object_type == "line"
                                && edge.height >= settings.edge_min_length_prefilter
                        })
                        .cloned(),
                );
            }
            TableStrategy::Text => {
                edges.extend(words_to_edges_v_cancellable(
                    &words,
                    settings.min_words_vertical,
                    cancellation,
                )?);
            }
            TableStrategy::Explicit => {}
        }

        for desc in &settings.explicit_vertical_lines {
            cancellation.check()?;
            match desc {
                ExplicitLine::Coord(x) => edges.push(EdgeObj {
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
                        edges.push(e.clone());
                    }
                }
                ExplicitLine::Rect(b) => {
                    edges.extend(
                        rect_to_edges(*b)
                            .into_iter()
                            .filter(|edge| edge.orientation == Some(Orientation::Vertical)),
                    );
                }
                ExplicitLine::Curve(pts) => {
                    edges.extend(
                        curve_to_edges(pts, "curve_edge")
                            .into_iter()
                            .filter(|edge| edge.orientation == Some(Orientation::Vertical)),
                    );
                }
            }
        }

        match h_strat {
            TableStrategy::Lines => {
                edges.extend(
                    self.edges
                        .iter()
                        .filter(|edge| {
                            edge.orientation == Some(Orientation::Horizontal)
                                && edge.width >= settings.edge_min_length_prefilter
                        })
                        .cloned(),
                );
            }
            TableStrategy::LinesStrict => {
                edges.extend(
                    self.edges
                        .iter()
                        .filter(|edge| {
                            edge.orientation == Some(Orientation::Horizontal)
                                && edge.object_type == "line"
                                && edge.width >= settings.edge_min_length_prefilter
                        })
                        .cloned(),
                );
            }
            TableStrategy::Text => {
                edges.extend(words_to_edges_h_cancellable(
                    &words,
                    settings.min_words_horizontal,
                    cancellation,
                )?);
            }
            TableStrategy::Explicit => {}
        }

        for desc in &settings.explicit_horizontal_lines {
            cancellation.check()?;
            match desc {
                ExplicitLine::Coord(y) => edges.push(EdgeObj {
                    x0: self.page_bbox.x0,
                    x1: self.page_bbox.x1,
                    top: *y,
                    bottom: *y,
                    width: self.page_bbox.x1 - self.page_bbox.x0,
                    height: 0.0,
                    orientation: Some(Orientation::Horizontal),
                    object_type: "explicit_edge",
                }),
                ExplicitLine::Edge(edge) => {
                    if edge.orientation == Some(Orientation::Horizontal) {
                        edges.push(edge.clone());
                    }
                }
                ExplicitLine::Rect(bbox) => {
                    edges.extend(
                        rect_to_edges(*bbox)
                            .into_iter()
                            .filter(|edge| edge.orientation == Some(Orientation::Horizontal)),
                    );
                }
                ExplicitLine::Curve(points) => {
                    edges.extend(
                        curve_to_edges(points, "curve_edge")
                            .into_iter()
                            .filter(|edge| edge.orientation == Some(Orientation::Horizontal)),
                    );
                }
            }
        }

        cancellation.check()?;
        *edges = merge_edges_cancellable(
            std::mem::take(edges),
            settings.snap_x_tolerance,
            settings.snap_y_tolerance,
            settings.join_x_tolerance,
            settings.join_y_tolerance,
            cancellation,
        )?;

        cancellation.check()?;
        edges.retain(|edge| {
            let length = if edge.orientation == Some(Orientation::Vertical) {
                edge.height
            } else {
                edge.width
            };
            length >= settings.edge_min_length
        });
        Ok(())
    }

    fn find_tables(&self) -> Vec<Table> {
        let mut workspace = TableWorkspace::default();
        self.find_tables_with_cancellation(&CancellationToken::new(), &mut workspace)
            .expect("a new cancellation token cannot be cancelled")
    }

    #[hotpath::measure]
    fn find_tables_with_cancellation(
        &self,
        cancellation: &CancellationToken,
        workspace: &mut TableWorkspace,
    ) -> Result<Vec<Table>> {
        self.get_edges_with_cancellation(cancellation, &mut workspace.edges)?;
        cancellation.check()?;
        let (store, intersections) = find_intersections(
            &workspace.edges,
            self.settings.intersection_x_tolerance,
            self.settings.intersection_y_tolerance,
            cancellation,
        )?;
        let cells = build_cells(&store, &intersections, cancellation)?;
        let tables = group_cells(cells, cancellation)?;
        let mut found = Vec::with_capacity(tables.len());
        for cell_group in tables {
            cancellation.check()?;
            found.push(Table { cells: cell_group });
        }
        Ok(found)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableCellMetadata {
    pub row_index: usize,
    pub column_index: usize,
    pub row_span: usize,
    pub column_span: usize,
    pub bbox: BBox,
    pub text: String,
    /// Where each run of `text` came from, when bidi reordering could have
    /// moved it. Empty when the layout path was used or runs were not asked
    /// for; never a guess.
    pub text_spans: Vec<TextSpan>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableMetadata {
    pub bbox: BBox,
    pub row_count: usize,
    pub column_count: usize,
    pub cells: Vec<TableCellMetadata>,
}

fn span_for_axis(axis_starts: &[f64], start_idx: usize, end_value: f64) -> usize {
    const EPS: f64 = 1e-6;
    let mut span = 0usize;
    for value in axis_starts.iter().skip(start_idx) {
        if *value < end_value - EPS {
            span += 1;
        } else {
            break;
        }
    }
    span.max(1)
}

#[cfg(test)]
fn table_to_metadata(
    table: &Table,
    chars: &[CharObj],
    text_settings: &TextSettings,
    arena: &dyn ArenaLookup,
) -> TableMetadata {
    table_to_metadata_with_cancellation(
        table,
        chars,
        text_settings,
        arena,
        &CancellationToken::new(),
    )
    .expect("a new cancellation token cannot be cancelled")
}

fn table_to_metadata_with_cancellation(
    table: &Table,
    chars: &[CharObj],
    text_settings: &TextSettings,
    arena: &dyn ArenaLookup,
    cancellation: &CancellationToken,
) -> Result<TableMetadata> {
    cancellation.check()?;
    let rows = table.rows();
    let row_count = rows.len();
    let column_count = rows.first().map(|row| row.cells.len()).unwrap_or(0);
    let (text_grid, spans_grid) =
        table.extract_soa_with_spans_cancellation(chars, text_settings, arena, cancellation)?;

    let mut row_starts: Vec<f64> = Vec::with_capacity(row_count);
    for (row_idx, row) in rows.iter().enumerate() {
        cancellation.check()?;
        let top = row
            .cells
            .iter()
            .flatten()
            .next()
            .map(|bbox| bbox.top)
            .unwrap_or(row_idx as f64);
        row_starts.push(top);
    }

    let mut column_starts: Vec<f64> = Vec::with_capacity(column_count);
    for col_idx in 0..column_count {
        cancellation.check()?;
        let mut start = None;
        for row in &rows {
            if let Some(Some(bbox)) = row.cells.get(col_idx) {
                start = Some(bbox.x0);
                break;
            }
        }
        column_starts.push(start.unwrap_or(col_idx as f64));
    }

    let mut cells = Vec::new();
    for (row_idx, row) in rows.iter().enumerate() {
        cancellation.check()?;
        for (col_idx, maybe_bbox) in row.cells.iter().enumerate() {
            let Some(bbox) = maybe_bbox else {
                continue;
            };
            let text = text_grid
                .get(row_idx)
                .and_then(|text_row| text_row.get(col_idx))
                .and_then(|value| value.clone())
                .unwrap_or_default();
            let text_spans = spans_grid
                .get(row_idx)
                .and_then(|spans_row| spans_row.get(col_idx))
                .and_then(|value| value.clone())
                .unwrap_or_default();
            cells.push(TableCellMetadata {
                row_index: row_idx,
                column_index: col_idx,
                row_span: span_for_axis(&row_starts, row_idx, bbox.bottom),
                column_span: span_for_axis(&column_starts, col_idx, bbox.x1),
                bbox: *bbox,
                text,
                text_spans,
            });
        }
    }

    Ok(TableMetadata {
        bbox: table.bbox(),
        row_count,
        column_count,
        cells,
    })
}

fn retain_tables_in_geometry(tables: &mut Vec<Table>, geom: &PageGeometry) {
    if !geom.force_crop {
        return;
    }

    let (x0, top, x1, bottom) = geom.page_bbox;
    let crop = BBox {
        x0,
        top,
        x1,
        bottom,
    };
    tables.retain(|table| bbox_overlap_strict(table.bbox(), crop));
}

/// Extract all tables from a page as nested vectors of cell text.
pub fn extract_tables_from_ltpage(
    page: &LTPage,
    geom: &PageGeometry,
    settings: &TableSettings,
) -> Vec<Vec<Vec<Option<String>>>> {
    let mut arena = PageArena::new();
    arena.reset();
    let finder = TableFinder::new(page, geom, settings, &mut arena);
    let mut tables = finder.find_tables();
    retain_tables_in_geometry(&mut tables, geom);
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
    extract_tables_from_objects_with_cancellation(
        chars,
        edges,
        geom,
        settings,
        arena,
        &CancellationToken::new(),
    )
    .expect("a new cancellation token cannot be cancelled")
}

/// Per-table run grid: rows of cells, each holding its output runs.
pub type TableTextSpans = Vec<Vec<Option<Vec<TextSpan>>>>;
type ExtractedTables = Vec<Vec<Vec<Option<String>>>>;

#[hotpath::measure]
pub(crate) fn extract_spans_borrowed(
    chars: &[CharObj],
    edges: &[EdgeObj],
    geom: &PageGeometry,
    settings: &TableSettings,
    arena: &impl ArenaLookup,
    cancellation: &CancellationToken,
    workspace: &mut TableWorkspace,
) -> Result<(ExtractedTables, Vec<TableTextSpans>)> {
    cancellation.check()?;
    let arena: &dyn ArenaLookup = arena;
    let finder = TableFinder::from_objects(chars, edges, geom, settings, arena);
    let mut tables = finder.find_tables_with_cancellation(cancellation, workspace)?;
    retain_tables_in_geometry(&mut tables, geom);
    let mut extracted = Vec::with_capacity(tables.len());
    let mut runs = Vec::with_capacity(tables.len());
    for table in &tables {
        cancellation.check()?;
        let (text, table_runs) = table.extract_soa_with_spans_cancellation(
            &finder.chars,
            &settings.text_settings,
            finder.arena,
            cancellation,
        )?;
        extracted.push(text);
        runs.push(table_runs);
    }
    Ok((extracted, runs))
}

#[hotpath::measure]
pub(crate) fn extract_tables_from_objects_with_cancellation(
    chars: Vec<CharObj>,
    edges: Vec<EdgeObj>,
    geom: &PageGeometry,
    settings: &TableSettings,
    arena: &impl ArenaLookup,
    cancellation: &CancellationToken,
) -> Result<Vec<Vec<Vec<Option<String>>>>> {
    let mut workspace = TableWorkspace::default();
    extract_tables_borrowed(
        &chars,
        &edges,
        geom,
        settings,
        arena,
        cancellation,
        &mut workspace,
    )
}

pub(crate) fn extract_tables_borrowed(
    chars: &[CharObj],
    edges: &[EdgeObj],
    geom: &PageGeometry,
    settings: &TableSettings,
    arena: &impl ArenaLookup,
    cancellation: &CancellationToken,
    workspace: &mut TableWorkspace,
) -> Result<Vec<Vec<Vec<Option<String>>>>> {
    cancellation.check()?;
    let arena: &dyn ArenaLookup = arena;
    let finder = TableFinder::from_objects(chars, edges, geom, settings, arena);
    let mut tables = finder.find_tables_with_cancellation(cancellation, workspace)?;
    retain_tables_in_geometry(&mut tables, geom);
    let mut extracted = Vec::with_capacity(tables.len());
    for table in &tables {
        cancellation.check()?;
        extracted.push(table.extract_soa_with_cancellation(
            &finder.chars,
            &settings.text_settings,
            finder.arena,
            cancellation,
        )?);
    }
    Ok(extracted)
}

/// Extract all tables from precomputed characters/edges with per-cell metadata.
pub fn extract_tables_with_metadata_from_objects(
    chars: Vec<CharObj>,
    edges: Vec<EdgeObj>,
    geom: &PageGeometry,
    settings: &TableSettings,
    arena: &impl ArenaLookup,
) -> Vec<TableMetadata> {
    extract_tables_with_metadata_from_objects_with_cancellation(
        chars,
        edges,
        geom,
        settings,
        arena,
        &CancellationToken::new(),
    )
    .expect("a new cancellation token cannot be cancelled")
}

#[hotpath::measure]
pub(crate) fn extract_tables_with_metadata_from_objects_with_cancellation(
    chars: Vec<CharObj>,
    edges: Vec<EdgeObj>,
    geom: &PageGeometry,
    settings: &TableSettings,
    arena: &impl ArenaLookup,
    cancellation: &CancellationToken,
) -> Result<Vec<TableMetadata>> {
    let mut workspace = TableWorkspace::default();
    extract_metadata_borrowed(
        &chars,
        &edges,
        geom,
        settings,
        arena,
        cancellation,
        &mut workspace,
    )
}

pub(crate) fn extract_metadata_borrowed(
    chars: &[CharObj],
    edges: &[EdgeObj],
    geom: &PageGeometry,
    settings: &TableSettings,
    arena: &impl ArenaLookup,
    cancellation: &CancellationToken,
    workspace: &mut TableWorkspace,
) -> Result<Vec<TableMetadata>> {
    cancellation.check()?;
    let arena: &dyn ArenaLookup = arena;
    let finder = TableFinder::from_objects(chars, edges, geom, settings, arena);
    let mut tables = finder.find_tables_with_cancellation(cancellation, workspace)?;
    retain_tables_in_geometry(&mut tables, geom);
    let mut metadata = Vec::with_capacity(tables.len());
    for table in &tables {
        cancellation.check()?;
        metadata.push(table_to_metadata_with_cancellation(
            table,
            &finder.chars,
            &settings.text_settings,
            finder.arena,
            cancellation,
        )?);
    }
    cancellation.check()?;
    Ok(metadata)
}

/// Extract the largest table from a page.
pub fn extract_table_from_ltpage(
    page: &LTPage,
    geom: &PageGeometry,
    settings: &TableSettings,
) -> Option<Vec<Vec<Option<String>>>> {
    let mut arena = PageArena::new();
    arena.reset();
    let finder = TableFinder::new(page, geom, settings, &mut arena);
    let mut tables = finder.find_tables();
    retain_tables_in_geometry(&mut tables, geom);
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
    let finder = TableFinder::from_objects(&chars, &edges, geom, settings, arena);
    let mut tables = finder.find_tables();
    retain_tables_in_geometry(&mut tables, geom);
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

/// Extract words from pre-collected character objects.
pub fn extract_words_from_objects(
    chars: Vec<CharObj>,
    settings: TextSettings,
    arena: &impl ArenaLookup,
) -> Vec<WordObj> {
    extract_words_from_objects_borrowed(chars, &settings, arena)
}

#[hotpath::measure]
pub(crate) fn extract_words_from_objects_borrowed(
    chars: Vec<CharObj>,
    settings: &TextSettings,
    arena: &impl ArenaLookup,
) -> Vec<WordObj> {
    let arena_lookup: &dyn ArenaLookup = arena;
    extract_words(&chars, settings, arena_lookup)
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

/// Extract text from pre-collected character objects.
pub fn extract_text_from_objects(
    chars: Vec<CharObj>,
    settings: TextSettings,
    arena: &impl ArenaLookup,
) -> String {
    extract_text_from_objects_borrowed(chars, &settings, arena)
}

#[hotpath::measure]
pub(crate) fn extract_text_from_objects_borrowed(
    chars: Vec<CharObj>,
    settings: &TextSettings,
    arena: &impl ArenaLookup,
) -> String {
    let arena_lookup: &dyn ArenaLookup = arena;
    extract_text(&chars, settings, arena_lookup)
}

#[cfg(test)]
mod tests {
    use super::collect_page_objects;
    use super::{Table, TableFinder, TableWorkspace, table_to_metadata};
    use crate::arena::types::{ArenaChar, ArenaItem, ArenaLine, ArenaPage, ArenaRect, ColorId};
    use crate::arena::{ArenaLookup, PageArena};
    use crate::cancellation::CancellationToken;
    use crate::error::PdfError;
    use crate::layout::table::collect_table_objects_from_arena;
    use crate::layout::table::types::{
        BBox, CharObj, PageGeometry, TableSettings, TableStrategy, TextSettings,
    };
    use crate::utils::Rect;
    use lasso::Spur;
    use std::cell::Cell;

    struct CancelLookup<'a> {
        arena: &'a PageArena,
        token: &'a CancellationToken,
        calls: Cell<usize>,
    }

    impl ArenaLookup for CancelLookup<'_> {
        fn resolve(&self, key: Spur) -> &str {
            let calls = self.calls.get() + 1;
            self.calls.set(calls);
            if calls == 1 {
                self.token.cancel();
            }
            self.arena.resolve(key)
        }

        fn color(&self, id: ColorId) -> &[f64] {
            self.arena.color(id)
        }
    }

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

    #[test]
    fn text_edges_stop_after_cancellation() {
        let mut arena = PageArena::new();
        let text = arena.intern("A");
        let chars = (0..1_024)
            .map(|index| CharObj {
                text,
                x0: index as f64,
                x1: index as f64 + 1.0,
                top: 0.0,
                bottom: 10.0,
                doctop: 0.0,
                width: 1.0,
                height: 10.0,
                size: 10.0,
                upright: true,
            })
            .collect::<Vec<_>>();
        let geometry = PageGeometry {
            page_bbox: (0.0, 0.0, 1_024.0, 10.0),
            mediabox: (0.0, 0.0, 1_024.0, 10.0),
            initial_doctop: 0.0,
            force_crop: false,
        };
        let settings = TableSettings {
            vertical_strategy: TableStrategy::Text,
            ..TableSettings::default()
        };
        let token = CancellationToken::new();
        let lookup = CancelLookup {
            arena: &arena,
            token: &token,
            calls: Cell::new(0),
        };
        let finder = TableFinder::from_objects(&chars, &[], &geometry, &settings, &lookup);
        let mut workspace = TableWorkspace::default();

        let result = finder.find_tables_with_cancellation(&token, &mut workspace);

        assert!(matches!(result, Err(PdfError::Cancelled)));
        assert!(lookup.calls.get() < chars.len());
    }

    #[test]
    fn table_metadata_reports_rowspan() {
        let mut arena = PageArena::new();
        arena.reset();

        let table = Table {
            cells: vec![
                BBox {
                    x0: 0.0,
                    top: 0.0,
                    x1: 5.0,
                    bottom: 20.0,
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

        let chars = vec![
            CharObj {
                text: arena.intern("A"),
                x0: 1.0,
                x1: 2.0,
                top: 9.0,
                bottom: 11.0,
                doctop: 9.0,
                width: 1.0,
                height: 2.0,
                size: 10.0,
                upright: true,
            },
            CharObj {
                text: arena.intern("B"),
                x0: 6.0,
                x1: 7.0,
                top: 4.0,
                bottom: 6.0,
                doctop: 4.0,
                width: 1.0,
                height: 2.0,
                size: 10.0,
                upright: true,
            },
            CharObj {
                text: arena.intern("C"),
                x0: 6.0,
                x1: 7.0,
                top: 14.0,
                bottom: 16.0,
                doctop: 14.0,
                width: 1.0,
                height: 2.0,
                size: 10.0,
                upright: true,
            },
        ];

        let metadata = table_to_metadata(&table, &chars, &TextSettings::default(), &arena);
        let spanning = metadata
            .cells
            .iter()
            .find(|cell| cell.row_index == 0 && cell.column_index == 0)
            .expect("top-left cell");
        assert_eq!(spanning.row_span, 2);
        assert_eq!(spanning.column_span, 1);
    }

    #[test]
    fn table_metadata_reports_colspan() {
        let mut arena = PageArena::new();
        arena.reset();

        let table = Table {
            cells: vec![
                BBox {
                    x0: 0.0,
                    top: 0.0,
                    x1: 10.0,
                    bottom: 10.0,
                },
                BBox {
                    x0: 0.0,
                    top: 10.0,
                    x1: 5.0,
                    bottom: 20.0,
                },
                BBox {
                    x0: 5.0,
                    top: 10.0,
                    x1: 10.0,
                    bottom: 20.0,
                },
            ],
        };

        let chars = vec![
            CharObj {
                text: arena.intern("A"),
                x0: 4.0,
                x1: 6.0,
                top: 4.0,
                bottom: 6.0,
                doctop: 4.0,
                width: 2.0,
                height: 2.0,
                size: 10.0,
                upright: true,
            },
            CharObj {
                text: arena.intern("B"),
                x0: 1.0,
                x1: 2.0,
                top: 14.0,
                bottom: 16.0,
                doctop: 14.0,
                width: 1.0,
                height: 2.0,
                size: 10.0,
                upright: true,
            },
            CharObj {
                text: arena.intern("C"),
                x0: 6.0,
                x1: 7.0,
                top: 14.0,
                bottom: 16.0,
                doctop: 14.0,
                width: 1.0,
                height: 2.0,
                size: 10.0,
                upright: true,
            },
        ];

        let metadata = table_to_metadata(&table, &chars, &TextSettings::default(), &arena);
        let spanning = metadata
            .cells
            .iter()
            .find(|cell| cell.row_index == 0 && cell.column_index == 0)
            .expect("top-left cell");
        assert_eq!(spanning.row_span, 1);
        assert_eq!(spanning.column_span, 2);
    }
}
