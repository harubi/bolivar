//! Table cell and grid construction from intersections.
//!
//! This module builds table cells from edge intersections and
//! groups connected cells into tables.

use std::collections::{HashMap, VecDeque};
use std::simd::prelude::*;

use super::intersections::{EdgeStore, IntersectionIdx};
use super::text::{extract_text_from_char_ids, extract_text_from_char_ids_layout};
use super::types::{
    BBox, BBoxKey, CharId, CharObj, KeyF64, KeyPoint, TextSettings, bbox_key, key_f64, key_point,
};

/// Convert intersections to table cells.
pub fn intersections_to_cells(
    store: &EdgeStore,
    intersections: &HashMap<KeyPoint, IntersectionIdx>,
) -> Vec<BBox> {
    const fn edge_id_key(edge_id: &BBoxKey) -> (u64, u64, u64, u64) {
        let BBoxKey(a, b, c, d) = *edge_id;
        (a, b, c, d)
    }

    fn sort_dedup_edge_ids(ids: &mut Vec<BBoxKey>) {
        ids.sort_by_key(edge_id_key);
        ids.dedup();
    }

    fn edge_lists_intersect(a: &[BBoxKey], b: &[BBoxKey]) -> bool {
        let mut i = 0usize;
        let mut j = 0usize;
        while i < a.len() && j < b.len() {
            let a_key = edge_id_key(&a[i]);
            let b_key = edge_id_key(&b[j]);
            if a_key == b_key {
                return true;
            }
            if a_key < b_key {
                i += 1;
            } else {
                j += 1;
            }
        }
        false
    }

    let mut points: Vec<KeyPoint> = intersections.keys().cloned().collect();
    points.sort();

    let mut point_index: HashMap<KeyPoint, usize> = HashMap::with_capacity(points.len());
    for (idx, point) in points.iter().enumerate() {
        point_index.insert(*point, idx);
    }

    let mut point_v_edges: Vec<Vec<BBoxKey>> = Vec::with_capacity(points.len());
    let mut point_h_edges: Vec<Vec<BBoxKey>> = Vec::with_capacity(points.len());
    for point in &points {
        let inter = intersections.get(point).unwrap();
        let mut v_ids: Vec<BBoxKey> = inter
            .v
            .iter()
            .map(|id| {
                let e = store.v(*id);
                bbox_key(&BBox {
                    x0: e.x0,
                    top: e.top,
                    x1: e.x1,
                    bottom: e.bottom,
                })
            })
            .collect();
        let mut h_ids: Vec<BBoxKey> = inter
            .h
            .iter()
            .map(|id| {
                let e = store.h(*id);
                bbox_key(&BBox {
                    x0: e.x0,
                    top: e.top,
                    x1: e.x1,
                    bottom: e.bottom,
                })
            })
            .collect();
        sort_dedup_edge_ids(&mut v_ids);
        sort_dedup_edge_ids(&mut h_ids);
        point_v_edges.push(v_ids);
        point_h_edges.push(h_ids);
    }

    let mut edge_points_v: HashMap<BBoxKey, Vec<usize>> = HashMap::new();
    let mut edge_points_h: HashMap<BBoxKey, Vec<usize>> = HashMap::new();
    for (pid, edges) in point_v_edges.iter().enumerate() {
        for edge_id in edges {
            edge_points_v.entry(*edge_id).or_default().push(pid);
        }
    }
    for (pid, edges) in point_h_edges.iter().enumerate() {
        for edge_id in edges {
            edge_points_h.entry(*edge_id).or_default().push(pid);
        }
    }

    for point_ids in edge_points_v.values_mut() {
        point_ids.sort_by(|a, b| points[*a].1.cmp(&points[*b].1));
        point_ids.dedup();
    }
    for point_ids in edge_points_h.values_mut() {
        point_ids.sort_by(|a, b| points[*a].0.cmp(&points[*b].0));
        point_ids.dedup();
    }

    let edge_connects = |p1: usize, p2: usize| -> bool {
        if points[p1].0 == points[p2].0 {
            return edge_lists_intersect(&point_v_edges[p1], &point_v_edges[p2]);
        }
        if points[p1].1 == points[p2].1 {
            return edge_lists_intersect(&point_h_edges[p1], &point_h_edges[p2]);
        }
        false
    };

    let mut cells = Vec::new();
    for (idx, point) in points.iter().enumerate() {
        let mut below_candidates: Vec<usize> = Vec::new();
        for edge_id in &point_v_edges[idx] {
            if let Some(point_ids) = edge_points_v.get(edge_id)
                && let Ok(pos) = point_ids.binary_search_by(|pid| points[*pid].1.cmp(&point.1))
            {
                below_candidates.extend(point_ids[pos + 1..].iter().copied());
            }
        }
        below_candidates.sort_by(|a, b| points[*a].1.cmp(&points[*b].1));
        below_candidates.dedup();

        let mut right_candidates: Vec<usize> = Vec::new();
        for edge_id in &point_h_edges[idx] {
            if let Some(point_ids) = edge_points_h.get(edge_id)
                && let Ok(pos) = point_ids.binary_search_by(|pid| points[*pid].0.cmp(&point.0))
            {
                right_candidates.extend(point_ids[pos + 1..].iter().copied());
            }
        }
        right_candidates.sort_by(|a, b| points[*a].0.cmp(&points[*b].0));
        right_candidates.dedup();

        'below: for below_id in below_candidates {
            if !edge_connects(idx, below_id) {
                continue;
            }
            for right_id in &right_candidates {
                if !edge_connects(idx, *right_id) {
                    continue;
                }
                let bottom_right = (points[*right_id].0, points[below_id].1);
                if let Some(&br_id) = point_index.get(&bottom_right)
                    && edge_connects(br_id, *right_id)
                    && edge_connects(br_id, below_id)
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
    cells
}

/// Group cells into connected tables using corner-sharing.
pub fn cells_to_tables_graph(cells: Vec<BBox>) -> Vec<Vec<BBox>> {
    const fn bbox_corners(b: &BBox) -> [KeyPoint; 4] {
        [
            key_point(b.x0, b.top),
            key_point(b.x0, b.bottom),
            key_point(b.x1, b.top),
            key_point(b.x1, b.bottom),
        ]
    }

    if cells.is_empty() {
        return Vec::new();
    }

    let mut corner_map: HashMap<KeyPoint, Vec<usize>> = HashMap::new();
    for (idx, cell) in cells.iter().enumerate() {
        for corner in bbox_corners(cell) {
            corner_map.entry(corner).or_default().push(idx);
        }
    }

    let mut visited = vec![false; cells.len()];
    let mut tables: Vec<Vec<BBox>> = Vec::new();
    let mut queue: VecDeque<usize> = VecDeque::new();

    for start in 0..cells.len() {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        queue.clear();
        queue.push_back(start);
        let mut group_indices = Vec::new();
        while let Some(idx) = queue.pop_front() {
            group_indices.push(idx);
            for corner in bbox_corners(&cells[idx]) {
                if let Some(neighbors) = corner_map.get(&corner) {
                    for &neighbor in neighbors {
                        if !visited[neighbor] {
                            visited[neighbor] = true;
                            queue.push_back(neighbor);
                        }
                    }
                }
            }
        }
        let mut group = Vec::with_capacity(group_indices.len());
        for idx in group_indices {
            group.push(cells[idx]);
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

    tables.into_iter().filter(|t| t.len() > 1).collect()
}

/// Convert cells to tables (wrapper for graph-based algorithm).
pub fn cells_to_tables(cells: Vec<BBox>) -> Vec<Vec<BBox>> {
    cells_to_tables_graph(cells)
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
    pub fn rows(&self) -> Vec<CellGroup> {
        self.get_rows_or_cols(true)
    }

    fn get_rows_or_cols(&self, rows: bool) -> Vec<CellGroup> {
        let axis = if rows { 0 } else { 1 };
        let antiaxis = if axis == 0 { 1 } else { 0 };
        let mut sorted = self.cells.clone();
        sorted.sort_by(|a, b| {
            let key_a = if antiaxis == 1 {
                (a.top, a.x0)
            } else {
                (a.x0, a.top)
            };
            let key_b = if antiaxis == 1 {
                (b.top, b.x0)
            } else {
                (b.x0, b.top)
            };
            key_a
                .partial_cmp(&key_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut xs: Vec<f64> = if axis == 0 {
            sorted.iter().map(|c| c.x0).collect()
        } else {
            sorted.iter().map(|c| c.top).collect()
        };
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        xs.dedup_by(|a, b| (*a - *b).abs() == 0.0);

        let mut grouped: Vec<Vec<BBox>> = Vec::new();
        let mut current: Vec<BBox> = Vec::new();
        let mut last_key: Option<f64> = None;
        for cell in sorted {
            let key = if antiaxis == 1 { cell.top } else { cell.x0 };
            if last_key.is_none() || (last_key.unwrap() - key).abs() < f64::EPSILON {
                current.push(cell);
            } else {
                grouped.push(current);
                current = vec![cell];
            }
            last_key = Some(key);
        }
        if !current.is_empty() {
            grouped.push(current);
        }

        let mut groups = Vec::new();
        for group in grouped {
            let mut map: HashMap<KeyF64, BBox> = HashMap::new();
            for cell in &group {
                let key = if axis == 0 { cell.x0 } else { cell.top };
                map.insert(key_f64(key), *cell);
            }
            let cells: Vec<Option<BBox>> =
                xs.iter().map(|x| map.get(&key_f64(*x)).cloned()).collect();
            groups.push(CellGroup { cells });
        }
        groups
    }

    /// Extract text from the table cells.
    pub fn extract(
        &self,
        chars: &[CharObj],
        text_settings: &TextSettings,
    ) -> Vec<Vec<Option<String>>> {
        let rows = self.rows();

        struct CellInfo {
            bbox: BBox,
        }

        let mut cell_infos: Vec<CellInfo> = Vec::new();
        let mut cell_id_grid: Vec<Vec<Option<usize>>> = Vec::with_capacity(rows.len());
        for row in &rows {
            let mut row_ids: Vec<Option<usize>> = Vec::with_capacity(row.cells.len());
            for cell in &row.cells {
                if let Some(bbox) = cell {
                    let id = cell_infos.len();
                    cell_infos.push(CellInfo { bbox: *bbox });
                    row_ids.push(Some(id));
                } else {
                    row_ids.push(None);
                }
            }
            cell_id_grid.push(row_ids);
        }

        let mut cell_char_indices: Vec<Vec<CharId>> = vec![Vec::new(); cell_infos.len()];

        enum CellEventKind {
            Add,
            Remove,
        }

        struct CellEvent {
            y: f64,
            kind: CellEventKind,
            cell_id: usize,
        }

        let mut events: Vec<CellEvent> = Vec::with_capacity(cell_infos.len() * 2);
        for (cell_id, info) in cell_infos.iter().enumerate() {
            events.push(CellEvent {
                y: info.bbox.top,
                kind: CellEventKind::Add,
                cell_id,
            });
            events.push(CellEvent {
                y: info.bbox.bottom,
                kind: CellEventKind::Remove,
                cell_id,
            });
        }

        let kind_order = |kind: &CellEventKind| match kind {
            CellEventKind::Add => 0,
            CellEventKind::Remove => 1,
        };
        events.sort_by(|a, b| {
            let y_cmp = a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal);
            if y_cmp != std::cmp::Ordering::Equal {
                return y_cmp;
            }
            kind_order(&a.kind)
                .cmp(&kind_order(&b.kind))
                .then(a.cell_id.cmp(&b.cell_id))
        });

        let mut chars_with_idx: Vec<(usize, f64, f64)> = Vec::with_capacity(chars.len());
        for (idx, ch) in chars.iter().enumerate() {
            let v_mid = (ch.top + ch.bottom) / 2.0;
            let h_mid = (ch.x0 + ch.x1) / 2.0;
            chars_with_idx.push((idx, v_mid, h_mid));
        }
        chars_with_idx.sort_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });

        let mut active: Vec<usize> = Vec::new();
        let mut active_x0: Vec<f64> = Vec::new();
        let mut active_x1: Vec<f64> = Vec::new();
        let mut active_top: Vec<f64> = Vec::new();
        let mut active_bottom: Vec<f64> = Vec::new();
        let mut active_pos: Vec<Option<usize>> = vec![None; cell_infos.len()];
        let mut event_idx = 0usize;
        for (char_idx, v_mid, h_mid) in chars_with_idx {
            while event_idx < events.len() {
                let event = &events[event_idx];
                if event.y > v_mid {
                    break;
                }
                match event.kind {
                    CellEventKind::Add => {
                        let bbox = &cell_infos[event.cell_id].bbox;
                        active_pos[event.cell_id] = Some(active.len());
                        active.push(event.cell_id);
                        active_x0.push(bbox.x0);
                        active_x1.push(bbox.x1);
                        active_top.push(bbox.top);
                        active_bottom.push(bbox.bottom);
                    }
                    CellEventKind::Remove => {
                        if let Some(pos) = active_pos[event.cell_id].take() {
                            let last = active.pop().unwrap();
                            let last_x0 = active_x0.pop().unwrap();
                            let last_x1 = active_x1.pop().unwrap();
                            let last_top = active_top.pop().unwrap();
                            let last_bottom = active_bottom.pop().unwrap();
                            if pos < active.len() {
                                active[pos] = last;
                                active_x0[pos] = last_x0;
                                active_x1[pos] = last_x1;
                                active_top[pos] = last_top;
                                active_bottom[pos] = last_bottom;
                                active_pos[last] = Some(pos);
                            }
                        }
                    }
                }
                event_idx += 1;
            }

            let ch = &chars[char_idx];
            let mut matches = 0usize;
            let mut i = 0usize;
            while i + 4 <= active.len() {
                let mask = char_in_bboxes_simd4(
                    h_mid,
                    v_mid,
                    [
                        active_x0[i],
                        active_x0[i + 1],
                        active_x0[i + 2],
                        active_x0[i + 3],
                    ],
                    [
                        active_x1[i],
                        active_x1[i + 1],
                        active_x1[i + 2],
                        active_x1[i + 3],
                    ],
                    [
                        active_top[i],
                        active_top[i + 1],
                        active_top[i + 2],
                        active_top[i + 3],
                    ],
                    [
                        active_bottom[i],
                        active_bottom[i + 1],
                        active_bottom[i + 2],
                        active_bottom[i + 3],
                    ],
                );
                if mask[0] {
                    let cell_id = active[i];
                    cell_char_indices[cell_id].push(CharId(char_idx));
                    matches += 1;
                }
                if mask[1] {
                    let cell_id = active[i + 1];
                    cell_char_indices[cell_id].push(CharId(char_idx));
                    matches += 1;
                }
                if mask[2] {
                    let cell_id = active[i + 2];
                    cell_char_indices[cell_id].push(CharId(char_idx));
                    matches += 1;
                }
                if mask[3] {
                    let cell_id = active[i + 3];
                    cell_char_indices[cell_id].push(CharId(char_idx));
                    matches += 1;
                }
                i += 4;
            }
            for &cell_id in &active[i..] {
                let bbox = &cell_infos[cell_id].bbox;
                if char_in_bbox(ch, bbox) {
                    cell_char_indices[cell_id].push(CharId(char_idx));
                    matches += 1;
                }
            }
            let _ = matches;
        }

        for indices in cell_char_indices.iter_mut() {
            indices.sort();
        }

        let mut table_arr = Vec::with_capacity(rows.len());
        for row_ids in cell_id_grid {
            let mut row_out: Vec<Option<String>> = Vec::with_capacity(row_ids.len());
            for cell_id in row_ids {
                if let Some(cell_id) = cell_id {
                    let indices = &cell_char_indices[cell_id];
                    if indices.is_empty() {
                        row_out.push(Some(String::new()));
                    } else {
                        let cell_bbox = &cell_infos[cell_id].bbox;
                        let text = if text_settings.layout {
                            extract_text_from_char_ids_layout(
                                chars,
                                indices,
                                text_settings,
                                cell_bbox,
                            )
                        } else {
                            extract_text_from_char_ids(chars, indices, text_settings)
                        };
                        row_out.push(Some(text));
                    }
                } else {
                    row_out.push(None);
                }
            }
            table_arr.push(row_out);
        }

        table_arr
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

#[inline]
pub(crate) fn char_in_bboxes_simd4(
    h_mid: f64,
    v_mid: f64,
    x0s: [f64; 4],
    x1s: [f64; 4],
    tops: [f64; 4],
    bottoms: [f64; 4],
) -> [bool; 4] {
    let hmid = Simd::<f64, 4>::splat(h_mid);
    let vmid = Simd::<f64, 4>::splat(v_mid);
    let x0v = Simd::<f64, 4>::from_array(x0s);
    let x1v = Simd::<f64, 4>::from_array(x1s);
    let topv = Simd::<f64, 4>::from_array(tops);
    let botv = Simd::<f64, 4>::from_array(bottoms);

    let x_ok = hmid.simd_ge(x0v) & hmid.simd_lt(x1v);
    let y_ok = vmid.simd_ge(topv) & vmid.simd_lt(botv);
    (x_ok & y_ok).to_array()
}

/// Check if a character's center is inside a bounding box.
fn char_in_bbox(c: &CharObj, bbox: &BBox) -> bool {
    let v_mid = (c.top + c.bottom) / 2.0;
    let h_mid = (c.x0 + c.x1) / 2.0;
    h_mid >= bbox.x0 && h_mid < bbox.x1 && v_mid >= bbox.top && v_mid < bbox.bottom
}
