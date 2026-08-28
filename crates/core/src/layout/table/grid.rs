//! Table cell and grid construction from intersections.
//!
//! This module builds table cells from edge intersections and
//! groups connected cells into tables.

use std::collections::{HashMap, VecDeque};
use std::num::NonZeroU32;

use super::intersections::{EdgeStore, IntersectionIdx};
use super::text::{
    TextSpan, extract_layout_from_id_iter, extract_spans_from_id_iter, extract_text_from_id_iter,
};
use super::types::{BBox, CharId, CharObj, HEdgeId, KeyPoint, TextSettings, VEdgeId, key_point};
use crate::arena::ArenaLookup;
use crate::cancellation::CancellationToken;
use crate::error::Result;

const CANCEL_INTERVAL: usize = 256;

/// Convert intersections to table cells.
#[cfg(test)]
pub fn intersections_to_cells(
    store: &EdgeStore,
    intersections: &HashMap<KeyPoint, IntersectionIdx>,
) -> Vec<BBox> {
    build_cells(store, intersections, &CancellationToken::new())
        .expect("a new cancellation token cannot be cancelled")
}

#[hotpath::measure]
pub(crate) fn build_cells(
    store: &EdgeStore,
    intersections: &HashMap<KeyPoint, IntersectionIdx>,
    cancellation: &CancellationToken,
) -> Result<Vec<BBox>> {
    fn edge_lists_intersect<T: Ord>(
        a: &[T],
        b: &[T],
        work: &mut usize,
        cancellation: &CancellationToken,
    ) -> Result<bool> {
        let mut i = 0usize;
        let mut j = 0usize;
        while i < a.len() && j < b.len() {
            *work += 1;
            if (*work).is_multiple_of(CANCEL_INTERVAL) {
                cancellation.check()?;
            }
            if a[i] == b[j] {
                return Ok(true);
            }
            if a[i] < b[j] {
                i += 1;
            } else {
                j += 1;
            }
        }
        Ok(false)
    }

    cancellation.check()?;
    let mut points: Vec<KeyPoint> = intersections.keys().cloned().collect();
    points.sort();
    cancellation.check()?;

    let mut point_index: HashMap<KeyPoint, usize> = HashMap::with_capacity(points.len());
    for (idx, point) in points.iter().enumerate() {
        if idx.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        point_index.insert(*point, idx);
    }

    let mut point_v_edges: Vec<Vec<VEdgeId>> = Vec::with_capacity(points.len());
    let mut point_h_edges: Vec<Vec<HEdgeId>> = Vec::with_capacity(points.len());
    for (index, point) in points.iter().enumerate() {
        if index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        let inter = intersections.get(point).unwrap();
        let v_ids = inter.v.clone();
        let h_ids = inter.h.clone();
        point_v_edges.push(v_ids);
        point_h_edges.push(h_ids);
    }

    let mut edge_points_v: Vec<Vec<usize>> = vec![Vec::new(); store.v.len()];
    let mut edge_points_h: Vec<Vec<usize>> = vec![Vec::new(); store.h.len()];
    for (pid, edges) in point_v_edges.iter().enumerate() {
        if pid.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        for edge_id in edges {
            edge_points_v[edge_id.index()].push(pid);
        }
    }
    for (pid, edges) in point_h_edges.iter().enumerate() {
        if pid.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        for edge_id in edges {
            edge_points_h[edge_id.index()].push(pid);
        }
    }

    for (index, point_ids) in edge_points_v.iter_mut().enumerate() {
        if index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        point_ids.sort_by(|a, b| points[*a].1.cmp(&points[*b].1));
        point_ids.dedup();
    }
    for (index, point_ids) in edge_points_h.iter_mut().enumerate() {
        if index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        point_ids.sort_by(|a, b| points[*a].0.cmp(&points[*b].0));
        point_ids.dedup();
    }

    let mut work = 0usize;
    let mut edge_connects = |p1: usize, p2: usize| -> Result<bool> {
        if points[p1].0 == points[p2].0 {
            return edge_lists_intersect(
                &point_v_edges[p1],
                &point_v_edges[p2],
                &mut work,
                cancellation,
            );
        }
        if points[p1].1 == points[p2].1 {
            return edge_lists_intersect(
                &point_h_edges[p1],
                &point_h_edges[p2],
                &mut work,
                cancellation,
            );
        }
        Ok(false)
    };

    let mut cells = Vec::new();
    for (idx, point) in points.iter().enumerate() {
        if idx.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        let mut below_candidates: Vec<usize> = Vec::new();
        for edge_id in &point_v_edges[idx] {
            let point_ids = &edge_points_v[edge_id.index()];
            if let Ok(pos) = point_ids.binary_search_by(|pid| points[*pid].1.cmp(&point.1)) {
                below_candidates.extend(point_ids[pos + 1..].iter().copied());
            }
        }
        below_candidates.sort_by(|a, b| points[*a].1.cmp(&points[*b].1));
        below_candidates.dedup();

        let mut right_candidates: Vec<usize> = Vec::new();
        for edge_id in &point_h_edges[idx] {
            let point_ids = &edge_points_h[edge_id.index()];
            if let Ok(pos) = point_ids.binary_search_by(|pid| points[*pid].0.cmp(&point.0)) {
                right_candidates.extend(point_ids[pos + 1..].iter().copied());
            }
        }
        right_candidates.sort_by(|a, b| points[*a].0.cmp(&points[*b].0));
        right_candidates.dedup();

        'below: for below_id in below_candidates {
            if !edge_connects(idx, below_id)? {
                continue;
            }
            for right_id in &right_candidates {
                if !edge_connects(idx, *right_id)? {
                    continue;
                }
                let bottom_right = (points[*right_id].0, points[below_id].1);
                if let Some(&br_id) = point_index.get(&bottom_right)
                    && edge_connects(br_id, *right_id)?
                    && edge_connects(br_id, below_id)?
                {
                    cells.push(BBox {
                        x0: point.0.into_inner(),
                        top: point.1.into_inner(),
                        x1: points[*right_id].0.into_inner(),
                        bottom: points[below_id].1.into_inner(),
                    });
                    break 'below;
                }
            }
        }
    }
    cancellation.check()?;
    Ok(cells)
}

#[hotpath::measure]
pub(crate) fn group_cells(
    cells: Vec<BBox>,
    cancellation: &CancellationToken,
) -> Result<Vec<Vec<BBox>>> {
    const fn bbox_corners(b: &BBox) -> [KeyPoint; 4] {
        [
            key_point(b.x0, b.top),
            key_point(b.x0, b.bottom),
            key_point(b.x1, b.top),
            key_point(b.x1, b.bottom),
        ]
    }

    cancellation.check()?;
    if cells.is_empty() {
        return Ok(Vec::new());
    }

    let mut corner_map: HashMap<KeyPoint, Vec<usize>> = HashMap::new();
    for (idx, cell) in cells.iter().enumerate() {
        if idx.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        for corner in bbox_corners(cell) {
            corner_map.entry(corner).or_default().push(idx);
        }
    }

    let mut visited = vec![false; cells.len()];
    let mut tables: Vec<Vec<BBox>> = Vec::new();
    let mut queue: VecDeque<usize> = VecDeque::new();
    let mut work = 0usize;

    for start in 0..cells.len() {
        if start.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        if visited[start] {
            continue;
        }
        visited[start] = true;
        queue.clear();
        queue.push_back(start);
        let mut group = Vec::new();
        while let Some(idx) = queue.pop_front() {
            group.push(cells[idx]);
            for corner in bbox_corners(&cells[idx]) {
                if let Some(neighbors) = corner_map.get(&corner) {
                    for &neighbor in neighbors {
                        work += 1;
                        if work.is_multiple_of(CANCEL_INTERVAL) {
                            cancellation.check()?;
                        }
                        if !visited[neighbor] {
                            visited[neighbor] = true;
                            queue.push_back(neighbor);
                        }
                    }
                }
            }
        }
        tables.push(group);
    }

    tables.sort_by(|a, b| {
        let min_a = a
            .iter()
            .map(|c| (c.top, c.x0))
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap();
        let min_b = b
            .iter()
            .map(|c| (c.top, c.x0))
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap();
        min_a
            .partial_cmp(&min_b)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    cancellation.check()?;

    Ok(tables.into_iter().filter(|t| t.len() > 1).collect())
}

/// Convert cells to tables (wrapper for graph-based algorithm).
#[cfg(test)]
pub fn cells_to_tables(cells: Vec<BBox>) -> Vec<Vec<BBox>> {
    group_cells(cells, &CancellationToken::new())
        .expect("a new cancellation token cannot be cancelled")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CellId(NonZeroU32);

impl CellId {
    fn from_index(index: usize) -> Self {
        let value = u32::try_from(index)
            .ok()
            .and_then(|value| value.checked_add(1))
            .expect("a table cannot contain more than u32::MAX cells");
        Self(NonZeroU32::new(value).expect("cell IDs start at one"))
    }

    fn index(self) -> usize {
        (self.0.get() - 1) as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CellMatch {
    cell: CellId,
    ch: CharId,
}

type TextGrid = Vec<Vec<Option<String>>>;
type SpanGrid = Vec<Vec<Option<Vec<TextSpan>>>>;

struct TableGrid {
    row_offsets: Vec<usize>,
    slots: Vec<Option<CellId>>,
    cells: Vec<BBox>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CellMode {
    Text,
    Spans,
}

struct ExtractedGrid {
    text: TextGrid,
    spans: Option<SpanGrid>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CellEventKind {
    Add,
    Remove,
}

struct CellEvent {
    y: f64,
    kind: CellEventKind,
    cell: CellId,
}

struct CharPoint {
    ch: CharId,
    vertical: f64,
    horizontal: f64,
}

/// A detected table with its cells.
pub struct Table {
    pub cells: Vec<BBox>,
}

impl Table {
    /// Get the bounding box of the entire table.
    pub fn bbox(&self) -> BBox {
        let mut x0 = f64::INFINITY;
        let mut top = f64::INFINITY;
        let mut x1 = f64::NEG_INFINITY;
        let mut bottom = f64::NEG_INFINITY;
        for c in &self.cells {
            x0 = x0.min(c.x0);
            top = top.min(c.top);
            x1 = x1.max(c.x1);
            bottom = bottom.max(c.bottom);
        }
        BBox {
            x0,
            top,
            x1,
            bottom,
        }
    }

    /// Get the rows of the table.
    #[hotpath::measure]
    pub fn rows(&self) -> Vec<CellGroup> {
        let grid = self.build_grid();
        grid.row_offsets
            .windows(2)
            .map(|row| CellGroup {
                cells: grid.slots[row[0]..row[1]]
                    .iter()
                    .map(|slot| slot.map(|id| grid.cells[id.index()]))
                    .collect(),
            })
            .collect()
    }

    fn build_grid(&self) -> TableGrid {
        let mut sorted = self.cells.clone();
        sorted.sort_by(|first, second| {
            (first.top, first.x0)
                .partial_cmp(&(second.top, second.x0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut columns: Vec<f64> = sorted.iter().map(|cell| cell.x0).collect();
        columns.sort_by(|first, second| {
            first
                .partial_cmp(second)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        columns.dedup_by(|first, second| (*first - *second).abs() == 0.0);

        let mut grid = TableGrid {
            row_offsets: vec![0],
            slots: Vec::new(),
            cells: Vec::with_capacity(sorted.len()),
        };
        let mut last_top: Option<f64> = None;
        for bbox in sorted {
            let starts_row = last_top
                .map(|top| (top - bbox.top).abs() >= f64::EPSILON)
                .unwrap_or(true);
            if starts_row {
                if last_top.is_some() {
                    grid.row_offsets.push(grid.slots.len());
                }
                grid.slots.resize(grid.slots.len() + columns.len(), None);
            }
            last_top = Some(bbox.top);

            let column = columns
                .binary_search_by(|value| {
                    value
                        .partial_cmp(&bbox.x0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .expect("each cell column comes from the column index");
            let row_start = *grid
                .row_offsets
                .last()
                .expect("the grid always has a first row offset");
            let slot = &mut grid.slots[row_start + column];
            if let Some(cell_id) = *slot {
                grid.cells[cell_id.index()] = bbox;
                continue;
            }

            let cell_id = CellId::from_index(grid.cells.len());
            grid.cells.push(bbox);
            *slot = Some(cell_id);
        }
        if last_top.is_some() {
            grid.row_offsets.push(grid.slots.len());
        }

        grid
    }

    /// Extract text from the table cells.
    pub fn extract(
        &self,
        chars: &[CharObj],
        text_settings: &TextSettings,
        arena: &dyn ArenaLookup,
    ) -> Vec<Vec<Option<String>>> {
        self.extract_soa(chars, text_settings, arena)
    }

    pub fn extract_soa(
        &self,
        chars: &[CharObj],
        text_settings: &TextSettings,
        arena: &dyn ArenaLookup,
    ) -> Vec<Vec<Option<String>>> {
        self.extract_soa_with_cancellation(chars, text_settings, arena, &CancellationToken::new())
            .expect("a new cancellation token cannot be cancelled")
    }

    pub(crate) fn extract_soa_with_cancellation(
        &self,
        chars: &[CharObj],
        text_settings: &TextSettings,
        arena: &dyn ArenaLookup,
        cancellation: &CancellationToken,
    ) -> Result<TextGrid> {
        Ok(self
            .extract_grid(chars, text_settings, arena, cancellation, CellMode::Text)?
            .text)
    }

    /// Extract cell text and the source word of each output run.
    pub(crate) fn extract_soa_with_spans_cancellation(
        &self,
        chars: &[CharObj],
        text_settings: &TextSettings,
        arena: &dyn ArenaLookup,
        cancellation: &CancellationToken,
    ) -> Result<(TextGrid, SpanGrid)> {
        let extracted =
            self.extract_grid(chars, text_settings, arena, cancellation, CellMode::Spans)?;
        Ok((
            extracted.text,
            extracted
                .spans
                .expect("span extraction always returns a span grid"),
        ))
    }

    #[hotpath::measure]
    fn extract_grid(
        &self,
        chars: &[CharObj],
        text_settings: &TextSettings,
        arena: &dyn ArenaLookup,
        cancellation: &CancellationToken,
        mode: CellMode,
    ) -> Result<ExtractedGrid> {
        cancellation.check()?;
        let grid = self.build_grid();

        let mut events: Vec<CellEvent> = Vec::with_capacity(grid.cells.len() * 2);
        for (index, bbox) in grid.cells.iter().enumerate() {
            let cell = CellId::from_index(index);
            events.push(CellEvent {
                y: bbox.top,
                kind: CellEventKind::Add,
                cell,
            });
            events.push(CellEvent {
                y: bbox.bottom,
                kind: CellEventKind::Remove,
                cell,
            });
        }
        events.sort_by(|a, b| {
            let y_cmp = a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal);
            if y_cmp != std::cmp::Ordering::Equal {
                return y_cmp;
            }
            a.kind.cmp(&b.kind).then(a.cell.cmp(&b.cell))
        });

        let mut char_points: Vec<CharPoint> = Vec::with_capacity(chars.len());
        for (idx, ch) in chars.iter().enumerate() {
            if idx.is_multiple_of(CANCEL_INTERVAL) {
                cancellation.check()?;
            }
            char_points.push(CharPoint {
                ch: CharId::from_index(idx),
                vertical: (ch.top + ch.bottom) / 2.0,
                horizontal: (ch.x0 + ch.x1) / 2.0,
            });
        }
        char_points.sort_by(|a, b| {
            a.vertical
                .partial_cmp(&b.vertical)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.ch.cmp(&b.ch))
        });

        const INACTIVE: usize = usize::MAX;
        let mut active: Vec<CellId> = Vec::new();
        let mut active_pos: Vec<usize> = vec![INACTIVE; grid.cells.len()];
        let mut matches: Vec<CellMatch> = Vec::with_capacity(chars.len());
        let mut event_idx = 0usize;
        for (processed, point) in char_points.into_iter().enumerate() {
            if processed.is_multiple_of(CANCEL_INTERVAL) {
                cancellation.check()?;
            }
            while event_idx < events.len() {
                let event = &events[event_idx];
                if event.y > point.vertical {
                    break;
                }
                match event.kind {
                    CellEventKind::Add => {
                        active_pos[event.cell.index()] = active.len();
                        active.push(event.cell);
                    }
                    CellEventKind::Remove => {
                        let pos = active_pos[event.cell.index()];
                        if pos != INACTIVE {
                            let last = active.pop().expect("an active cell has an entry");
                            active_pos[event.cell.index()] = INACTIVE;
                            if pos < active.len() {
                                active[pos] = last;
                                active_pos[last.index()] = pos;
                            }
                        }
                    }
                }
                event_idx += 1;
            }

            for &cell in &active {
                let bbox = &grid.cells[cell.index()];
                if point.horizontal >= bbox.x0
                    && point.horizontal < bbox.x1
                    && point.vertical >= bbox.top
                    && point.vertical < bbox.bottom
                {
                    matches.push(CellMatch { cell, ch: point.ch });
                }
            }
        }

        cancellation.check()?;
        matches.sort_unstable();

        let row_count = grid.row_offsets.len().saturating_sub(1);
        let mut table_arr = Vec::with_capacity(row_count);
        let mut spans_arr = (mode == CellMode::Spans).then(|| Vec::with_capacity(row_count));
        let mut match_index = 0usize;
        for row in grid.row_offsets.windows(2) {
            cancellation.check()?;
            let slots = &grid.slots[row[0]..row[1]];
            let mut row_out: Vec<Option<String>> = Vec::with_capacity(slots.len());
            let mut row_spans = spans_arr.as_ref().map(|_| Vec::with_capacity(slots.len()));
            for slot in slots {
                let Some(cell_id) = *slot else {
                    row_out.push(None);
                    if let Some(spans) = &mut row_spans {
                        spans.push(None);
                    }
                    continue;
                };

                let bbox = &grid.cells[cell_id.index()];
                while match_index < matches.len() && matches[match_index].cell < cell_id {
                    match_index += 1;
                }
                let match_start = match_index;
                while match_index < matches.len() && matches[match_index].cell == cell_id {
                    match_index += 1;
                }
                let cell_matches = &matches[match_start..match_index];
                if cell_matches.is_empty() {
                    row_out.push(Some(String::new()));
                    if let Some(spans) = &mut row_spans {
                        spans.push(Some(Vec::new()));
                    }
                    continue;
                }

                let (text, spans) = if text_settings.layout {
                    (
                        extract_layout_from_id_iter(
                            chars,
                            cell_matches.iter().map(|entry| entry.ch),
                            text_settings,
                            bbox,
                            arena,
                        ),
                        Vec::new(),
                    )
                } else if mode == CellMode::Spans {
                    extract_spans_from_id_iter(
                        chars,
                        cell_matches.iter().map(|entry| entry.ch),
                        text_settings,
                        arena,
                    )
                } else {
                    (
                        extract_text_from_id_iter(
                            chars,
                            cell_matches.iter().map(|entry| entry.ch),
                            text_settings,
                            arena,
                        ),
                        Vec::new(),
                    )
                };
                row_out.push(Some(text));
                if let Some(row_spans) = &mut row_spans {
                    row_spans.push(Some(spans));
                }
            }
            table_arr.push(row_out);
            if let (Some(spans_arr), Some(row_spans)) = (&mut spans_arr, row_spans) {
                spans_arr.push(row_spans);
            }
        }
        debug_assert_eq!(match_index, matches.len());

        cancellation.check()?;
        Ok(ExtractedGrid {
            text: table_arr,
            spans: spans_arr,
        })
    }
}

/// A group of cells in a row or column.
pub struct CellGroup {
    pub cells: Vec<Option<BBox>>,
}

impl CellGroup {
    #[allow(dead_code)]
    pub fn bbox(&self) -> BBox {
        let cells: Vec<BBox> = self.cells.iter().filter_map(|c| *c).collect();
        let mut x0 = f64::INFINITY;
        let mut top = f64::INFINITY;
        let mut x1 = f64::NEG_INFINITY;
        let mut bottom = f64::NEG_INFINITY;
        for c in cells {
            x0 = x0.min(c.x0);
            top = top.min(c.top);
            x1 = x1.max(c.x1);
            bottom = bottom.max(c.bottom);
        }
        BBox {
            x0,
            top,
            x1,
            bottom,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BBox, CellMatch, CharId, HEdgeId, Table, VEdgeId};

    #[test]
    fn flat_grid_builds_sparse_rows() {
        let table = Table {
            cells: vec![
                BBox {
                    x0: 0.0,
                    top: 0.0,
                    x1: 5.0,
                    bottom: 10.0,
                },
                BBox {
                    x0: 5.0,
                    top: 0.0,
                    x1: 10.0,
                    bottom: 5.0,
                },
                BBox {
                    x0: 5.0,
                    top: 5.0,
                    x1: 10.0,
                    bottom: 10.0,
                },
            ],
        };

        let grid = table.build_grid();
        let flat_rows: Vec<Vec<Option<BBox>>> = grid
            .row_offsets
            .windows(2)
            .map(|range| {
                grid.slots[range[0]..range[1]]
                    .iter()
                    .map(|slot| slot.map(|id| grid.cells[id.index()]))
                    .collect()
            })
            .collect();

        assert_eq!(
            flat_rows,
            vec![
                vec![Some(table.cells[0]), Some(table.cells[1])],
                vec![None, Some(table.cells[2])],
            ]
        );
    }

    #[test]
    fn cell_match_uses_compact_ids() {
        assert_eq!(std::mem::size_of::<CellMatch>(), 8);
        assert_eq!(std::mem::size_of::<CharId>(), 4);
        assert_eq!(std::mem::size_of::<VEdgeId>(), 4);
        assert_eq!(std::mem::size_of::<HEdgeId>(), 4);
    }
}
