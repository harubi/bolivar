//! Table extraction (ported from pdfplumber.table)

use std::collections::{BTreeMap, HashMap, VecDeque};

use ordered_float::OrderedFloat;

use crate::layout::{
    LTChar, LTCurve, LTItem, LTLine, LTPage, LTRect, LTTextLineHorizontal, LTTextLineVertical,
    TextBoxType, TextLineElement, TextLineType,
};
use crate::utils::{Point, Rect};

const DEFAULT_SNAP_TOLERANCE: f64 = 3.0;
const DEFAULT_JOIN_TOLERANCE: f64 = 3.0;
const DEFAULT_MIN_WORDS_VERTICAL: usize = 3;
const DEFAULT_MIN_WORDS_HORIZONTAL: usize = 1;

const DEFAULT_X_TOLERANCE: f64 = 3.0;
const DEFAULT_Y_TOLERANCE: f64 = 3.0;
const DEFAULT_X_DENSITY: f64 = 7.25;
const DEFAULT_Y_DENSITY: f64 = 13.0;

type KeyF64 = OrderedFloat<f64>;
type KeyPoint = (KeyF64, KeyF64);

fn key_f64(v: f64) -> KeyF64 {
    OrderedFloat(v)
}

fn key_point(x: f64, y: f64) -> KeyPoint {
    (OrderedFloat(x), OrderedFloat(y))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextDir {
    Ttb,
    Btt,
    Ltr,
    Rtl,
}

impl TextDir {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "ttb" => Some(TextDir::Ttb),
            "btt" => Some(TextDir::Btt),
            "ltr" => Some(TextDir::Ltr),
            "rtl" => Some(TextDir::Rtl),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BBox {
    pub x0: f64,
    pub top: f64,
    pub x1: f64,
    pub bottom: f64,
}

impl BBox {
    fn width(&self) -> f64 {
        self.x1 - self.x0
    }
    fn height(&self) -> f64 {
        self.bottom - self.top
    }
}

#[derive(Clone, Debug)]
struct CharObj {
    text: String,
    x0: f64,
    x1: f64,
    top: f64,
    bottom: f64,
    doctop: f64,
    width: f64,
    height: f64,
    size: f64,
    upright: bool,
}

#[derive(Clone, Debug)]
pub struct WordObj {
    pub text: String,
    pub x0: f64,
    pub x1: f64,
    pub top: f64,
    pub bottom: f64,
    pub doctop: f64,
    pub width: f64,
    pub height: f64,
    pub upright: bool,
    pub direction: TextDir,
}

#[derive(Clone, Debug)]
pub struct EdgeObj {
    pub x0: f64,
    pub x1: f64,
    pub top: f64,
    pub bottom: f64,
    pub width: f64,
    pub height: f64,
    pub orientation: Option<Orientation>,
    pub object_type: &'static str,
}

#[derive(Clone, Debug)]
pub struct TableSettings {
    pub vertical_strategy: String,
    pub horizontal_strategy: String,
    pub explicit_vertical_lines: Vec<ExplicitLine>,
    pub explicit_horizontal_lines: Vec<ExplicitLine>,
    pub snap_x_tolerance: f64,
    pub snap_y_tolerance: f64,
    pub join_x_tolerance: f64,
    pub join_y_tolerance: f64,
    pub edge_min_length: f64,
    pub edge_min_length_prefilter: f64,
    pub min_words_vertical: usize,
    pub min_words_horizontal: usize,
    pub intersection_x_tolerance: f64,
    pub intersection_y_tolerance: f64,
    pub text_settings: TextSettings,
}

impl Default for TableSettings {
    fn default() -> Self {
        Self {
            vertical_strategy: "lines".to_string(),
            horizontal_strategy: "lines".to_string(),
            explicit_vertical_lines: Vec::new(),
            explicit_horizontal_lines: Vec::new(),
            snap_x_tolerance: DEFAULT_SNAP_TOLERANCE,
            snap_y_tolerance: DEFAULT_SNAP_TOLERANCE,
            join_x_tolerance: DEFAULT_JOIN_TOLERANCE,
            join_y_tolerance: DEFAULT_JOIN_TOLERANCE,
            edge_min_length: 3.0,
            edge_min_length_prefilter: 1.0,
            min_words_vertical: DEFAULT_MIN_WORDS_VERTICAL,
            min_words_horizontal: DEFAULT_MIN_WORDS_HORIZONTAL,
            intersection_x_tolerance: 3.0,
            intersection_y_tolerance: 3.0,
            text_settings: TextSettings::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum ExplicitLine {
    Coord(f64),
    Edge(EdgeObj),
    Rect(BBox),
    Curve(Vec<Point>),
}

#[derive(Clone, Debug)]
pub struct TextSettings {
    pub x_tolerance: f64,
    pub y_tolerance: f64,
    pub x_tolerance_ratio: Option<f64>,
    pub y_tolerance_ratio: Option<f64>,
    pub keep_blank_chars: bool,
    pub use_text_flow: bool,
    pub vertical_ttb: bool,
    pub horizontal_ltr: bool,
    pub line_dir: TextDir,
    pub char_dir: TextDir,
    pub line_dir_rotated: Option<TextDir>,
    pub char_dir_rotated: Option<TextDir>,
    pub split_at_punctuation: String,
    pub expand_ligatures: bool,
    pub layout: bool,
}

impl Default for TextSettings {
    fn default() -> Self {
        Self {
            x_tolerance: DEFAULT_X_TOLERANCE,
            y_tolerance: DEFAULT_Y_TOLERANCE,
            x_tolerance_ratio: None,
            y_tolerance_ratio: None,
            keep_blank_chars: false,
            use_text_flow: false,
            vertical_ttb: true,
            horizontal_ltr: true,
            line_dir: TextDir::Ttb,
            char_dir: TextDir::Ltr,
            line_dir_rotated: None,
            char_dir_rotated: None,
            split_at_punctuation: String::new(),
            expand_ligatures: true,
            layout: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PageGeometry {
    pub page_bbox: Rect,
    pub mediabox: Rect,
    pub initial_doctop: f64,
    pub force_crop: bool,
}

fn page_height(geom: &PageGeometry) -> f64 {
    geom.mediabox.3 - geom.mediabox.1
}

fn mb_offsets(geom: &PageGeometry) -> (f64, f64) {
    (geom.mediabox.0, geom.mediabox.1)
}

fn to_top_left_y(y: f64, geom: &PageGeometry) -> f64 {
    let (.., mb_top) = mb_offsets(geom);
    page_height(geom) - y + mb_top
}

fn to_top_left_bbox(x0: f64, y0: f64, x1: f64, y1: f64, geom: &PageGeometry) -> BBox {
    let (mb_x0, mb_top) = mb_offsets(geom);
    let top = (page_height(geom) - y1) + mb_top;
    let bottom = (page_height(geom) - y0) + mb_top;
    BBox {
        x0: x0 + mb_x0,
        x1: x1 + mb_x0,
        top,
        bottom,
    }
}

fn bbox_from_chars(chars: &[&CharObj]) -> BBox {
    let mut x0 = f64::INFINITY;
    let mut top = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut bottom = f64::NEG_INFINITY;
    for c in chars {
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

fn bbox_from_words(words: &[WordObj]) -> BBox {
    let mut x0 = f64::INFINITY;
    let mut top = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut bottom = f64::NEG_INFINITY;
    for w in words {
        x0 = x0.min(w.x0);
        top = top.min(w.top);
        x1 = x1.max(w.x1);
        bottom = bottom.max(w.bottom);
    }
    BBox {
        x0,
        top,
        x1,
        bottom,
    }
}

fn bbox_overlap(a: BBox, b: BBox) -> Option<BBox> {
    let o_left = a.x0.max(b.x0);
    let o_right = a.x1.min(b.x1);
    let o_top = a.top.max(b.top);
    let o_bottom = a.bottom.min(b.bottom);
    let o_width = o_right - o_left;
    let o_height = o_bottom - o_top;
    if o_height >= 0.0 && o_width >= 0.0 && (o_height + o_width) > 0.0 {
        Some(BBox {
            x0: o_left,
            top: o_top,
            x1: o_right,
            bottom: o_bottom,
        })
    } else {
        None
    }
}

fn bbox_overlap_strict(a: BBox, b: BBox) -> bool {
    match bbox_overlap(a, b) {
        Some(overlap) => overlap.width() > 0.0 && overlap.height() > 0.0,
        None => false,
    }
}

fn clip_edge_to_bbox(edge: EdgeObj, crop: BBox) -> Option<EdgeObj> {
    let bbox = BBox {
        x0: edge.x0,
        top: edge.top,
        x1: edge.x1,
        bottom: edge.bottom,
    };
    let overlap = bbox_overlap(bbox, crop)?;
    Some(EdgeObj {
        x0: overlap.x0,
        x1: overlap.x1,
        top: overlap.top,
        bottom: overlap.bottom,
        width: overlap.width(),
        height: overlap.height(),
        orientation: edge.orientation,
        object_type: edge.object_type,
    })
}

fn cluster_list(mut xs: Vec<f64>, tolerance: f64) -> Vec<Vec<f64>> {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if tolerance == 0.0 || xs.len() < 2 {
        return xs.into_iter().map(|x| vec![x]).collect();
    }
    let mut groups: Vec<Vec<f64>> = Vec::new();
    let mut current: Vec<f64> = Vec::new();
    let mut last = xs[0];
    current.push(xs[0]);
    for x in xs.into_iter().skip(1) {
        if x <= last + tolerance {
            current.push(x);
        } else {
            groups.push(current);
            current = vec![x];
        }
        last = x;
    }
    groups.push(current);
    groups
}

fn make_cluster_dict(values: Vec<f64>, tolerance: f64) -> HashMap<KeyF64, usize> {
    let mut unique: Vec<f64> = values;
    unique.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    unique.dedup_by(|a, b| (*a - *b).abs() == 0.0);
    let clusters = cluster_list(unique, tolerance);
    let mut dict = HashMap::new();
    for (i, cluster) in clusters.into_iter().enumerate() {
        for val in cluster {
            dict.insert(key_f64(val), i);
        }
    }
    dict
}

fn cluster_objects<T: Clone, F: Fn(&T) -> f64>(
    xs: &[T],
    key_fn: F,
    tolerance: f64,
    preserve_order: bool,
) -> Vec<Vec<T>> {
    let values: Vec<f64> = xs.iter().map(|x| key_fn(x)).collect();
    let cluster_dict = make_cluster_dict(values, tolerance);

    let mut cluster_tuples: Vec<(T, usize)> = if preserve_order {
        xs.iter()
            .map(|x| {
                (
                    x.clone(),
                    *cluster_dict.get(&key_f64(key_fn(x))).unwrap_or(&0),
                )
            })
            .collect()
    } else {
        let mut tuples: Vec<(T, usize)> = xs
            .iter()
            .map(|x| {
                (
                    x.clone(),
                    *cluster_dict.get(&key_f64(key_fn(x))).unwrap_or(&0),
                )
            })
            .collect();
        tuples.sort_by(|a, b| a.1.cmp(&b.1));
        tuples
    };

    let mut groups: Vec<Vec<T>> = Vec::new();
    let mut current: Vec<T> = Vec::new();
    let mut last_idx: Option<usize> = None;
    for (item, idx) in cluster_tuples.drain(..) {
        if last_idx.is_none() || last_idx.unwrap() == idx {
            current.push(item);
        } else {
            groups.push(current);
            current = vec![item];
        }
        last_idx = Some(idx);
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

fn move_edge(edge: &EdgeObj, axis: Orientation, value: f64) -> EdgeObj {
    match axis {
        Orientation::Horizontal => EdgeObj {
            x0: edge.x0 + value,
            x1: edge.x1 + value,
            ..edge.clone()
        },
        Orientation::Vertical => EdgeObj {
            top: edge.top + value,
            bottom: edge.bottom + value,
            ..edge.clone()
        },
    }
}

fn snap_edges(edges: &[EdgeObj], x_tolerance: f64, y_tolerance: f64) -> Vec<EdgeObj> {
    let mut v_edges: Vec<EdgeObj> = edges
        .iter()
        .filter(|e| e.orientation == Some(Orientation::Vertical))
        .cloned()
        .collect();
    let mut h_edges: Vec<EdgeObj> = edges
        .iter()
        .filter(|e| e.orientation == Some(Orientation::Horizontal))
        .cloned()
        .collect();

    if x_tolerance > 0.0 {
        let clusters = cluster_objects(&v_edges, |e| e.x0, x_tolerance, false);
        let mut snapped: Vec<EdgeObj> = Vec::new();
        for cluster in clusters {
            let avg = cluster.iter().map(|e| e.x0).sum::<f64>() / (cluster.len() as f64);
            for e in cluster {
                snapped.push(move_edge(&e, Orientation::Horizontal, avg - e.x0));
            }
        }
        v_edges = snapped;
    }

    if y_tolerance > 0.0 {
        let clusters = cluster_objects(&h_edges, |e| e.top, y_tolerance, false);
        let mut snapped: Vec<EdgeObj> = Vec::new();
        for cluster in clusters {
            let avg = cluster.iter().map(|e| e.top).sum::<f64>() / (cluster.len() as f64);
            for e in cluster {
                snapped.push(move_edge(&e, Orientation::Vertical, avg - e.top));
            }
        }
        h_edges = snapped;
    }

    v_edges.into_iter().chain(h_edges.into_iter()).collect()
}

fn join_edge_group(edges: &[EdgeObj], orientation: Orientation, tolerance: f64) -> Vec<EdgeObj> {
    let mut sorted = edges.to_vec();
    sorted.sort_by(|a, b| {
        let a_min = if orientation == Orientation::Horizontal {
            a.x0
        } else {
            a.top
        };
        let b_min = if orientation == Orientation::Horizontal {
            b.x0
        } else {
            b.top
        };
        a_min
            .partial_cmp(&b_min)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut joined: Vec<EdgeObj> = Vec::new();
    if sorted.is_empty() {
        return joined;
    }
    joined.push(sorted[0].clone());
    for e in sorted.into_iter().skip(1) {
        let last = joined.last_mut().unwrap();
        let e_min = if orientation == Orientation::Horizontal {
            e.x0
        } else {
            e.top
        };
        let e_max = if orientation == Orientation::Horizontal {
            e.x1
        } else {
            e.bottom
        };
        let last_max = if orientation == Orientation::Horizontal {
            last.x1
        } else {
            last.bottom
        };
        if e_min <= last_max + tolerance {
            if e_max > last_max {
                if orientation == Orientation::Horizontal {
                    last.x1 = e.x1;
                    last.width = last.x1 - last.x0;
                } else {
                    last.bottom = e.bottom;
                    last.height = last.bottom - last.top;
                }
            }
        } else {
            joined.push(e);
        }
    }
    joined
}

fn merge_edges(
    edges: Vec<EdgeObj>,
    snap_x_tolerance: f64,
    snap_y_tolerance: f64,
    join_x_tolerance: f64,
    join_y_tolerance: f64,
) -> Vec<EdgeObj> {
    let mut edges = edges;
    if snap_x_tolerance > 0.0 || snap_y_tolerance > 0.0 {
        edges = snap_edges(&edges, snap_x_tolerance, snap_y_tolerance);
    }

    // Group by orientation and position (match pdfplumber exact grouping)
    let mut grouped: BTreeMap<(Orientation, OrderedFloat<f64>), Vec<EdgeObj>> = BTreeMap::new();
    for e in &edges {
        let orientation = match e.orientation {
            Some(o) => o,
            None => continue,
        };
        let key_val = match orientation {
            Orientation::Horizontal => e.top,
            Orientation::Vertical => e.x0,
        };
        let key = (orientation, OrderedFloat(key_val));
        grouped.entry(key).or_default().push(e.clone());
    }

    let mut merged: Vec<EdgeObj> = Vec::new();
    for ((orientation, _), group) in grouped {
        let tol = if orientation == Orientation::Horizontal {
            join_x_tolerance
        } else {
            join_y_tolerance
        };
        merged.extend(join_edge_group(&group, orientation, tol));
    }

    merged
}

fn filter_edges(
    edges: Vec<EdgeObj>,
    orientation: Option<Orientation>,
    edge_type: Option<&str>,
    min_length: f64,
) -> Vec<EdgeObj> {
    edges
        .into_iter()
        .filter(|e| {
            let dim = if e.orientation == Some(Orientation::Vertical) {
                e.height
            } else {
                e.width
            };
            let et_ok = match edge_type {
                Some(t) => e.object_type == t,
                None => true,
            };
            let orient_ok = match orientation {
                Some(o) => e.orientation == Some(o),
                None => true,
            };
            et_ok && orient_ok && dim >= min_length
        })
        .collect()
}

fn line_to_edge(line: &EdgeObj) -> EdgeObj {
    let orientation = if (line.top - line.bottom).abs() < f64::EPSILON {
        Some(Orientation::Horizontal)
    } else {
        Some(Orientation::Vertical)
    };
    EdgeObj {
        orientation,
        ..line.clone()
    }
}

fn rect_to_edges(rect: BBox) -> Vec<EdgeObj> {
    let top = EdgeObj {
        x0: rect.x0,
        x1: rect.x1,
        top: rect.top,
        bottom: rect.top,
        width: rect.x1 - rect.x0,
        height: 0.0,
        orientation: Some(Orientation::Horizontal),
        object_type: "rect_edge",
    };
    let bottom = EdgeObj {
        x0: rect.x0,
        x1: rect.x1,
        top: rect.bottom,
        bottom: rect.bottom,
        width: rect.x1 - rect.x0,
        height: 0.0,
        orientation: Some(Orientation::Horizontal),
        object_type: "rect_edge",
    };
    let left = EdgeObj {
        x0: rect.x0,
        x1: rect.x0,
        top: rect.top,
        bottom: rect.bottom,
        width: 0.0,
        height: rect.bottom - rect.top,
        orientation: Some(Orientation::Vertical),
        object_type: "rect_edge",
    };
    let right = EdgeObj {
        x0: rect.x1,
        x1: rect.x1,
        top: rect.top,
        bottom: rect.bottom,
        width: 0.0,
        height: rect.bottom - rect.top,
        orientation: Some(Orientation::Vertical),
        object_type: "rect_edge",
    };
    vec![top, bottom, left, right]
}

fn curve_to_edges(points: &[Point], object_type: &'static str) -> Vec<EdgeObj> {
    let mut edges = Vec::new();
    for pair in points.windows(2) {
        let p0 = pair[0];
        let p1 = pair[1];
        let x0 = p0.0.min(p1.0);
        let x1 = p0.0.max(p1.0);
        let top = p0.1.min(p1.1);
        let bottom = p0.1.max(p1.1);
        let orientation = if (p0.0 - p1.0).abs() < f64::EPSILON {
            Some(Orientation::Vertical)
        } else if (p0.1 - p1.1).abs() < f64::EPSILON {
            Some(Orientation::Horizontal)
        } else {
            None
        };
        edges.push(EdgeObj {
            x0,
            x1,
            top,
            bottom,
            width: (x1 - x0).abs(),
            height: (bottom - top).abs(),
            orientation,
            object_type,
        });
    }
    edges
}

fn words_to_edges_h(words: &[WordObj], word_threshold: usize) -> Vec<EdgeObj> {
    let clusters = cluster_objects(words, |w| w.top, 1.0, false);
    let large_clusters = clusters
        .into_iter()
        .filter(|c| c.len() >= word_threshold)
        .collect::<Vec<_>>();
    let mut rects: Vec<BBox> = large_clusters.iter().map(|c| bbox_from_words(c)).collect();
    if rects.is_empty() {
        return Vec::new();
    }
    let min_x0 = rects.iter().map(|r| r.x0).fold(f64::INFINITY, f64::min);
    let max_x1 = rects.iter().map(|r| r.x1).fold(f64::NEG_INFINITY, f64::max);

    let mut edges = Vec::new();
    for r in rects.drain(..) {
        edges.push(EdgeObj {
            x0: min_x0,
            x1: max_x1,
            top: r.top,
            bottom: r.top,
            width: max_x1 - min_x0,
            height: 0.0,
            orientation: Some(Orientation::Horizontal),
            object_type: "word_edge",
        });
        edges.push(EdgeObj {
            x0: min_x0,
            x1: max_x1,
            top: r.bottom,
            bottom: r.bottom,
            width: max_x1 - min_x0,
            height: 0.0,
            orientation: Some(Orientation::Horizontal),
            object_type: "word_edge",
        });
    }
    edges
}

fn words_to_edges_v(words: &[WordObj], word_threshold: usize) -> Vec<EdgeObj> {
    let by_x0 = cluster_objects(words, |w| w.x0, 1.0, false);
    let by_x1 = cluster_objects(words, |w| w.x1, 1.0, false);
    let by_center = cluster_objects(words, |w| (w.x0 + w.x1) / 2.0, 1.0, false);

    let mut clusters = Vec::new();
    clusters.extend(by_x0);
    clusters.extend(by_x1);
    clusters.extend(by_center);

    clusters.sort_by(|a, b| b.len().cmp(&a.len()));
    let large_clusters: Vec<Vec<WordObj>> = clusters
        .into_iter()
        .filter(|c| c.len() >= word_threshold)
        .collect();

    let bboxes: Vec<BBox> = large_clusters.iter().map(|c| bbox_from_words(c)).collect();

    let mut condensed: Vec<BBox> = Vec::new();
    'outer: for bbox in bboxes {
        for c in &condensed {
            if bbox_overlap(bbox, *c).is_some() {
                continue 'outer;
            }
        }
        condensed.push(bbox);
    }

    if condensed.is_empty() {
        return Vec::new();
    }

    condensed.sort_by(|a, b| a.x0.partial_cmp(&b.x0).unwrap_or(std::cmp::Ordering::Equal));

    let max_x1 = condensed
        .iter()
        .map(|r| r.x1)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_top = condensed
        .iter()
        .map(|r| r.top)
        .fold(f64::INFINITY, f64::min);
    let max_bottom = condensed
        .iter()
        .map(|r| r.bottom)
        .fold(f64::NEG_INFINITY, f64::max);

    let mut edges = Vec::new();
    for r in condensed {
        edges.push(EdgeObj {
            x0: r.x0,
            x1: r.x0,
            top: min_top,
            bottom: max_bottom,
            width: 0.0,
            height: max_bottom - min_top,
            orientation: Some(Orientation::Vertical),
            object_type: "word_edge",
        });
    }
    edges.push(EdgeObj {
        x0: max_x1,
        x1: max_x1,
        top: min_top,
        bottom: max_bottom,
        width: 0.0,
        height: max_bottom - min_top,
        orientation: Some(Orientation::Vertical),
        object_type: "word_edge",
    });
    edges
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
struct VEdgeId(usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
struct HEdgeId(usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
struct CharId(usize);

struct EdgeStore {
    v: Vec<EdgeObj>,
    h: Vec<EdgeObj>,
}

impl EdgeStore {
    fn v(&self, id: VEdgeId) -> &EdgeObj {
        &self.v[id.0]
    }

    fn h(&self, id: HEdgeId) -> &EdgeObj {
        &self.h[id.0]
    }
}

#[derive(Clone, Debug)]
struct IntersectionIdx {
    v: Vec<VEdgeId>,
    h: Vec<HEdgeId>,
}

fn edges_to_intersections(
    edges: &[EdgeObj],
    x_tol: f64,
    y_tol: f64,
) -> (EdgeStore, HashMap<KeyPoint, IntersectionIdx>) {
    enum EventKind {
        AddV,
        QueryH,
        RemoveV,
    }

    struct Event {
        y: f64,
        kind: EventKind,
        idx: usize,
    }

    let mut v_sorted: Vec<EdgeObj> = edges
        .iter()
        .filter(|e| e.orientation == Some(Orientation::Vertical))
        .cloned()
        .collect();
    let mut h_sorted: Vec<EdgeObj> = edges
        .iter()
        .filter(|e| e.orientation == Some(Orientation::Horizontal))
        .cloned()
        .collect();

    v_sorted.sort_by(|a, b| {
        a.x0.partial_cmp(&b.x0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                a.top
                    .partial_cmp(&b.top)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
    h_sorted.sort_by(|a, b| {
        a.top
            .partial_cmp(&b.top)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.x0.partial_cmp(&b.x0).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut events = Vec::with_capacity(v_sorted.len() * 2 + h_sorted.len());
    for (idx, v) in v_sorted.iter().enumerate() {
        events.push(Event {
            y: v.top - y_tol,
            kind: EventKind::AddV,
            idx,
        });
        events.push(Event {
            y: v.bottom + y_tol,
            kind: EventKind::RemoveV,
            idx,
        });
    }
    for (idx, h) in h_sorted.iter().enumerate() {
        events.push(Event {
            y: h.top,
            kind: EventKind::QueryH,
            idx,
        });
    }

    let kind_order = |kind: &EventKind| match kind {
        EventKind::AddV => 0,
        EventKind::QueryH => 1,
        EventKind::RemoveV => 2,
    };

    events.sort_by(|a, b| {
        let y_cmp = a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal);
        if y_cmp != std::cmp::Ordering::Equal {
            return y_cmp;
        }
        let kind_cmp = kind_order(&a.kind).cmp(&kind_order(&b.kind));
        if kind_cmp != std::cmp::Ordering::Equal {
            return kind_cmp;
        }
        let (ax0, atop) = match a.kind {
            EventKind::AddV | EventKind::RemoveV => {
                let v = &v_sorted[a.idx];
                (v.x0, v.top)
            }
            EventKind::QueryH => {
                let h = &h_sorted[a.idx];
                (h.x0, h.top)
            }
        };
        let (bx0, btop) = match b.kind {
            EventKind::AddV | EventKind::RemoveV => {
                let v = &v_sorted[b.idx];
                (v.x0, v.top)
            }
            EventKind::QueryH => {
                let h = &h_sorted[b.idx];
                (h.x0, h.top)
            }
        };
        ax0.partial_cmp(&bx0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(atop.partial_cmp(&btop).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.idx.cmp(&b.idx))
    });

    let mut active: BTreeMap<KeyF64, Vec<usize>> = BTreeMap::new();
    let mut pairs: HashMap<KeyPoint, Vec<(VEdgeId, HEdgeId)>> = HashMap::new();

    for event in events {
        match event.kind {
            EventKind::AddV => {
                let v = &v_sorted[event.idx];
                active.entry(key_f64(v.x0)).or_default().push(event.idx);
            }
            EventKind::RemoveV => {
                let v = &v_sorted[event.idx];
                let key = key_f64(v.x0);
                if let Some(bucket) = active.get_mut(&key) {
                    if let Some(pos) = bucket.iter().position(|&idx| idx == event.idx) {
                        bucket.remove(pos);
                    }
                    if bucket.is_empty() {
                        active.remove(&key);
                    }
                }
            }
            EventKind::QueryH => {
                let h = &h_sorted[event.idx];
                let x_min = key_f64(h.x0 - x_tol);
                let x_max = key_f64(h.x1 + x_tol);
                for (_x0, v_indices) in active.range(x_min..=x_max) {
                    for &v_idx in v_indices {
                        let v = &v_sorted[v_idx];
                        if v.top <= h.top + y_tol
                            && v.bottom >= h.top - y_tol
                            && v.x0 >= h.x0 - x_tol
                            && v.x0 <= h.x1 + x_tol
                        {
                            let vertex = key_point(v.x0, h.top);
                            pairs
                                .entry(vertex)
                                .or_default()
                                .push((VEdgeId(v_idx), HEdgeId(event.idx)));
                        }
                    }
                }
            }
        }
    }

    let mut intersections: HashMap<KeyPoint, IntersectionIdx> = HashMap::with_capacity(pairs.len());
    for (vertex, mut pair_list) in pairs {
        pair_list.sort_by(|a, b| a.0.0.cmp(&b.0.0).then(a.1.0.cmp(&b.1.0)));
        let mut v = Vec::with_capacity(pair_list.len());
        let mut h = Vec::with_capacity(pair_list.len());
        for (v_idx, h_idx) in pair_list {
            v.push(v_idx);
            h.push(h_idx);
        }
        intersections.insert(vertex, IntersectionIdx { v, h });
    }
    (
        EdgeStore {
            v: v_sorted,
            h: h_sorted,
        },
        intersections,
    )
}

#[derive(Hash, Eq, PartialEq, Clone, Copy, Debug)]
struct BBoxKey(u64, u64, u64, u64);

fn bbox_key(b: &BBox) -> BBoxKey {
    BBoxKey(
        b.x0.to_bits(),
        b.top.to_bits(),
        b.x1.to_bits(),
        b.bottom.to_bits(),
    )
}

fn intersections_to_cells(
    store: &EdgeStore,
    intersections: &HashMap<KeyPoint, IntersectionIdx>,
) -> Vec<BBox> {
    fn edge_id_key(edge_id: &BBoxKey) -> (u64, u64, u64, u64) {
        let BBoxKey(a, b, c, d) = *edge_id;
        (a, b, c, d)
    }

    fn sort_dedup_edge_ids(ids: &mut Vec<BBoxKey>) {
        ids.sort_by(|a, b| edge_id_key(a).cmp(&edge_id_key(b)));
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
            if let Some(point_ids) = edge_points_v.get(edge_id) {
                if let Ok(pos) = point_ids.binary_search_by(|pid| points[*pid].1.cmp(&point.1)) {
                    below_candidates.extend(point_ids[pos + 1..].iter().copied());
                }
            }
        }
        below_candidates.sort_by(|a, b| points[*a].1.cmp(&points[*b].1));
        below_candidates.dedup();

        let mut right_candidates: Vec<usize> = Vec::new();
        for edge_id in &point_h_edges[idx] {
            if let Some(point_ids) = edge_points_h.get(edge_id) {
                if let Ok(pos) = point_ids.binary_search_by(|pid| points[*pid].0.cmp(&point.0)) {
                    right_candidates.extend(point_ids[pos + 1..].iter().copied());
                }
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
                if let Some(&br_id) = point_index.get(&bottom_right) {
                    if edge_connects(br_id, *right_id) && edge_connects(br_id, below_id) {
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
    }
    cells
}

fn cells_to_tables_graph(cells: Vec<BBox>) -> Vec<Vec<BBox>> {
    fn bbox_corners(b: &BBox) -> [KeyPoint; 4] {
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
            group.push(cells[idx].clone());
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

fn cells_to_tables(cells: Vec<BBox>) -> Vec<Vec<BBox>> {
    cells_to_tables_graph(cells)
}

struct Table {
    cells: Vec<BBox>,
}

impl Table {
    fn bbox(&self) -> BBox {
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

    fn rows(&self) -> Vec<CellGroup> {
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
                map.insert(key_f64(key), cell.clone());
            }
            let cells: Vec<Option<BBox>> =
                xs.iter().map(|x| map.get(&key_f64(*x)).cloned()).collect();
            groups.push(CellGroup { cells });
        }
        groups
    }

    fn extract(&self, chars: &[CharObj], text_settings: &TextSettings) -> Vec<Vec<Option<String>>> {
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
        let mut active_pos: Vec<Option<usize>> = vec![None; cell_infos.len()];
        let mut event_idx = 0usize;
        for (char_idx, v_mid, _h_mid) in chars_with_idx {
            while event_idx < events.len() {
                let event = &events[event_idx];
                if event.y > v_mid {
                    break;
                }
                match event.kind {
                    CellEventKind::Add => {
                        active_pos[event.cell_id] = Some(active.len());
                        active.push(event.cell_id);
                    }
                    CellEventKind::Remove => {
                        if let Some(pos) = active_pos[event.cell_id].take() {
                            let last = active.pop().unwrap();
                            if pos < active.len() {
                                active[pos] = last;
                                active_pos[last] = Some(pos);
                            }
                        }
                    }
                }
                event_idx += 1;
            }

            let ch = &chars[char_idx];
            let mut matches = 0usize;
            for &cell_id in &active {
                let bbox = &cell_infos[cell_id].bbox;
                if char_in_bbox(ch, bbox) {
                    cell_char_indices[cell_id].push(CharId(char_idx));
                    matches += 1;
                }
            }
            debug_assert!(matches <= 1, "char matched multiple cells");
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
                        let text = extract_text_from_char_ids(chars, indices, text_settings);
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

struct CellGroup {
    cells: Vec<Option<BBox>>,
}

impl CellGroup {
    fn bbox(&self) -> BBox {
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

fn char_in_bbox(c: &CharObj, bbox: &BBox) -> bool {
    let v_mid = (c.top + c.bottom) / 2.0;
    let h_mid = (c.x0 + c.x1) / 2.0;
    h_mid >= bbox.x0 && h_mid < bbox.x1 && v_mid >= bbox.top && v_mid < bbox.bottom
}

fn get_line_cluster_key(dir: TextDir, obj: &WordObj) -> f64 {
    match dir {
        TextDir::Ttb => obj.top,
        TextDir::Btt => -obj.bottom,
        TextDir::Ltr => obj.x0,
        TextDir::Rtl => -obj.x1,
    }
}

fn get_char_sort_key(dir: TextDir, obj: &CharObj) -> (f64, f64) {
    match dir {
        TextDir::Ttb => (obj.top, obj.bottom),
        TextDir::Btt => (-(obj.top + obj.height), -obj.top),
        TextDir::Ltr => (obj.x0, obj.x0),
        TextDir::Rtl => (-obj.x1, -obj.x0),
    }
}

fn get_char_dir(upright: bool, settings: &TextSettings) -> TextDir {
    if !upright && !settings.vertical_ttb {
        return TextDir::Btt;
    }
    if upright && !settings.horizontal_ltr {
        return TextDir::Rtl;
    }
    if upright {
        settings.char_dir
    } else {
        settings.char_dir_rotated.unwrap_or(settings.line_dir)
    }
}

fn merge_chars(ordered: &[&CharObj], settings: &TextSettings) -> WordObj {
    let bbox = bbox_from_chars(ordered);
    let doctop_adj = ordered[0].doctop - ordered[0].top;
    let upright = ordered[0].upright;
    let char_dir = get_char_dir(upright, settings);

    let text = ordered
        .iter()
        .map(|c| expand_ligature(&c.text, settings.expand_ligatures))
        .collect::<String>();

    WordObj {
        text,
        x0: bbox.x0,
        x1: bbox.x1,
        top: bbox.top,
        bottom: bbox.bottom,
        doctop: bbox.top + doctop_adj,
        height: bbox.height(),
        width: bbox.width(),
        upright,
        direction: char_dir,
    }
}

fn expand_ligature(text: &str, expand: bool) -> String {
    if !expand {
        return text.to_string();
    }
    match text {
        "\u{fb00}" => "ff".to_string(),
        "\u{fb03}" => "ffi".to_string(),
        "\u{fb04}" => "ffl".to_string(),
        "\u{fb01}" => "fi".to_string(),
        "\u{fb02}" => "fl".to_string(),
        "\u{fb06}" => "st".to_string(),
        "\u{fb05}" => "st".to_string(),
        _ => text.to_string(),
    }
}

fn char_begins_new_word(
    prev: &CharObj,
    curr: &CharObj,
    direction: TextDir,
    x_tolerance: f64,
    y_tolerance: f64,
) -> bool {
    let (x, y, ay, cy, ax, bx, cx) = match direction {
        TextDir::Ltr => (
            x_tolerance,
            y_tolerance,
            prev.top,
            curr.top,
            prev.x0,
            prev.x1,
            curr.x0,
        ),
        TextDir::Rtl => (
            x_tolerance,
            y_tolerance,
            prev.top,
            curr.top,
            -prev.x1,
            -prev.x0,
            -curr.x1,
        ),
        TextDir::Ttb => (
            y_tolerance,
            x_tolerance,
            prev.x0,
            curr.x0,
            prev.top,
            prev.bottom,
            curr.top,
        ),
        TextDir::Btt => (
            y_tolerance,
            x_tolerance,
            prev.x0,
            curr.x0,
            -prev.bottom,
            -prev.top,
            -curr.bottom,
        ),
    };

    (cx < ax) || (cx > bx + x) || ((cy - ay).abs() > y)
}

fn iter_chars_to_words<'a>(
    ordered: &'a [&'a CharObj],
    direction: TextDir,
    settings: &TextSettings,
) -> Vec<Vec<&'a CharObj>> {
    let mut words: Vec<Vec<&CharObj>> = Vec::new();
    let mut current: Vec<&CharObj> = Vec::new();

    let xt = settings.x_tolerance;
    let yt = settings.y_tolerance;
    let xtr = settings.x_tolerance_ratio;
    let ytr = settings.y_tolerance_ratio;

    for &char in ordered {
        let text = &char.text;
        if !settings.keep_blank_chars && text.chars().all(|c| c.is_whitespace()) {
            if !current.is_empty() {
                words.push(current);
                current = Vec::new();
            }
        } else if settings.split_at_punctuation.contains(text) {
            if !current.is_empty() {
                words.push(current);
            }
            words.push(vec![char]);
            current = Vec::new();
        } else if !current.is_empty() {
            let prev = current.last().unwrap();
            let xtol = xtr.map(|r| r * prev.size).unwrap_or(xt);
            let ytol = ytr.map(|r| r * prev.size).unwrap_or(yt);
            if char_begins_new_word(prev, char, direction, xtol, ytol) {
                words.push(current);
                current = vec![char];
            } else {
                current.push(char);
            }
        } else {
            current.push(char);
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn iter_chars_to_lines<'a>(
    chars: &'a [&'a CharObj],
    settings: &TextSettings,
) -> Vec<(Vec<&'a CharObj>, TextDir)> {
    let upright = chars.first().map(|c| c.upright).unwrap_or(true);
    let line_dir = if upright {
        settings.line_dir
    } else {
        settings.line_dir_rotated.unwrap_or(settings.char_dir)
    };
    let char_dir = get_char_dir(upright, settings);

    let line_cluster_key = |c: &&CharObj| match line_dir {
        TextDir::Ttb => c.top,
        TextDir::Btt => -c.bottom,
        TextDir::Ltr => c.x0,
        TextDir::Rtl => -c.x1,
    };

    let char_sort_key = |c: &&CharObj| get_char_sort_key(char_dir, c);

    let tolerance = if matches!(line_dir, TextDir::Ttb | TextDir::Btt) {
        settings.y_tolerance
    } else {
        settings.x_tolerance
    };

    let subclusters = cluster_objects(chars, line_cluster_key, tolerance, false);
    let mut out = Vec::new();
    for sc in subclusters {
        let mut sorted = sc;
        sorted.sort_by(|a, b| {
            let ka = char_sort_key(a);
            let kb = char_sort_key(b);
            ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
        });
        out.push((sorted, char_dir));
    }
    out
}

fn extract_words(chars: &[CharObj], settings: &TextSettings) -> Vec<WordObj> {
    if chars.is_empty() {
        return Vec::new();
    }
    let refs: Vec<&CharObj> = chars.iter().collect();
    extract_words_refs(&refs, settings)
}

fn extract_words_refs<'a>(chars: &'a [&'a CharObj], settings: &TextSettings) -> Vec<WordObj> {
    if chars.is_empty() {
        return Vec::new();
    }
    let mut grouped: HashMap<(bool, String), Vec<&CharObj>> = HashMap::new();
    for &c in chars {
        let key = (c.upright, String::new());
        grouped.entry(key).or_default().push(c);
    }

    let mut words = Vec::new();
    for (_key, group) in grouped {
        let line_groups = if settings.use_text_flow {
            vec![(group.clone(), settings.char_dir)]
        } else {
            iter_chars_to_lines(&group, settings)
        };
        for (line_chars, direction) in line_groups {
            for word_chars in iter_chars_to_words(&line_chars, direction, settings) {
                words.push(merge_chars(&word_chars, settings));
            }
        }
    }
    words
}

fn textmap_to_string(lines: Vec<String>, line_dir: TextDir, char_dir: TextDir) -> String {
    let mut lines = lines;
    if matches!(line_dir, TextDir::Btt | TextDir::Rtl) {
        lines.reverse();
    }
    if char_dir == TextDir::Rtl {
        lines = lines
            .into_iter()
            .map(|l| l.chars().rev().collect::<String>())
            .collect();
    }
    if matches!(line_dir, TextDir::Rtl | TextDir::Ltr) {
        let max_len = lines.iter().map(|l| l.len()).max().unwrap_or(0);
        let mut new_lines = Vec::new();
        for line in lines {
            if char_dir == TextDir::Btt {
                new_lines.push(format!("{}{}", " ".repeat(max_len - line.len()), line));
            } else {
                new_lines.push(format!("{}{}", line, " ".repeat(max_len - line.len())));
            }
        }
        let mut out = String::new();
        for i in 0..max_len {
            for line in &new_lines {
                out.push(line.chars().nth(i).unwrap_or(' '));
            }
            if i + 1 < max_len {
                out.push('\n');
            }
        }
        return out;
    }
    lines.join("\n")
}

fn extract_text(chars: &[CharObj], settings: &TextSettings) -> String {
    if chars.is_empty() {
        return String::new();
    }
    let refs: Vec<&CharObj> = chars.iter().collect();
    extract_text_refs(&refs, settings)
}

fn extract_text_refs(chars: &[&CharObj], settings: &TextSettings) -> String {
    if chars.is_empty() {
        return String::new();
    }
    let words = extract_words_refs(chars, settings);

    let line_dir_render = settings.line_dir;
    let char_dir_render = settings.char_dir;

    let line_cluster_key = |w: &WordObj| get_line_cluster_key(settings.line_dir, w);
    let tolerance = if matches!(line_dir_render, TextDir::Ttb | TextDir::Btt) {
        settings.y_tolerance
    } else {
        settings.x_tolerance
    };

    let lines = cluster_objects(&words, line_cluster_key, tolerance, false);

    let mut line_texts = Vec::new();
    for line in lines {
        let mut line_sorted = line;
        line_sorted.sort_by(|a, b| {
            let key_a = match char_dir_render {
                TextDir::Ltr => a.x0,
                TextDir::Rtl => -a.x1,
                TextDir::Ttb => a.top,
                TextDir::Btt => -a.bottom,
            };
            let key_b = match char_dir_render {
                TextDir::Ltr => b.x0,
                TextDir::Rtl => -b.x1,
                TextDir::Ttb => b.top,
                TextDir::Btt => -b.bottom,
            };
            key_a
                .partial_cmp(&key_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let line_str = line_sorted
            .iter()
            .map(|w| w.text.clone())
            .collect::<Vec<_>>()
            .join(" ");
        line_texts.push(line_str);
    }

    textmap_to_string(line_texts, line_dir_render, char_dir_render)
}

fn extract_text_from_char_ids(
    chars: &[CharObj],
    ids: &[CharId],
    settings: &TextSettings,
) -> String {
    if ids.is_empty() {
        return String::new();
    }
    let mut refs: Vec<&CharObj> = Vec::with_capacity(ids.len());
    for id in ids {
        refs.push(&chars[id.0]);
    }
    extract_text_refs(&refs, settings)
}

fn rects_equal(a: Rect, b: Rect) -> bool {
    const EPS: f64 = 1e-6;
    (a.0 - b.0).abs() < EPS
        && (a.1 - b.1).abs() < EPS
        && (a.2 - b.2).abs() < EPS
        && (a.3 - b.3).abs() < EPS
}

fn collect_page_objects(page: &LTPage, geom: &PageGeometry) -> (Vec<CharObj>, Vec<EdgeObj>) {
    let mut chars: Vec<CharObj> = Vec::new();
    let mut edges: Vec<EdgeObj> = Vec::new();

    fn visit_item(
        item: &LTItem,
        geom: &PageGeometry,
        crop_bbox: Option<BBox>,
        chars: &mut Vec<CharObj>,
        edges: &mut Vec<EdgeObj>,
    ) {
        match item {
            LTItem::Char(c) => {
                let bbox = to_top_left_bbox(c.x0(), c.y0(), c.x1(), c.y1(), geom);
                let bbox = if let Some(crop) = crop_bbox {
                    let Some(bbox) = bbox_overlap(bbox, crop) else {
                        return;
                    };
                    bbox
                } else {
                    bbox
                };
                let text = c.get_text().to_string();
                let size = c.size();
                let upright = c.upright();
                let width = bbox.width();
                let height = bbox.height();
                let doctop = geom.initial_doctop + bbox.top;
                chars.push(CharObj {
                    text,
                    x0: bbox.x0,
                    x1: bbox.x1,
                    top: bbox.top,
                    bottom: bbox.bottom,
                    doctop,
                    width,
                    height,
                    size,
                    upright,
                });
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
                let bbox = to_top_left_bbox(r.x0(), r.y0(), r.x1(), r.y1(), geom);
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
            LTItem::TextLine(line) => match line {
                TextLineType::Horizontal(l) => {
                    for el in l.iter() {
                        if let TextLineElement::Char(c) = el {
                            let bbox = to_top_left_bbox(c.x0(), c.y0(), c.x1(), c.y1(), geom);
                            let bbox = if let Some(crop) = crop_bbox {
                                let Some(bbox) = bbox_overlap(bbox, crop) else {
                                    continue;
                                };
                                bbox
                            } else {
                                bbox
                            };
                            let text = c.get_text().to_string();
                            let size = c.size();
                            let upright = c.upright();
                            let width = bbox.width();
                            let height = bbox.height();
                            let doctop = geom.initial_doctop + bbox.top;
                            chars.push(CharObj {
                                text,
                                x0: bbox.x0,
                                x1: bbox.x1,
                                top: bbox.top,
                                bottom: bbox.bottom,
                                doctop,
                                width,
                                height,
                                size,
                                upright,
                            });
                        }
                    }
                }
                TextLineType::Vertical(l) => {
                    for el in l.iter() {
                        if let TextLineElement::Char(c) = el {
                            let bbox = to_top_left_bbox(c.x0(), c.y0(), c.x1(), c.y1(), geom);
                            let bbox = if let Some(crop) = crop_bbox {
                                let Some(bbox) = bbox_overlap(bbox, crop) else {
                                    continue;
                                };
                                bbox
                            } else {
                                bbox
                            };
                            let text = c.get_text().to_string();
                            let size = c.size();
                            let upright = c.upright();
                            let width = bbox.width();
                            let height = bbox.height();
                            let doctop = geom.initial_doctop + bbox.top;
                            chars.push(CharObj {
                                text,
                                x0: bbox.x0,
                                x1: bbox.x1,
                                top: bbox.top,
                                bottom: bbox.bottom,
                                doctop,
                                width,
                                height,
                                size,
                                upright,
                            });
                        }
                    }
                }
            },
            LTItem::TextBox(tb) => match tb {
                TextBoxType::Horizontal(b) => {
                    for line in b.iter() {
                        for el in line.iter() {
                            if let TextLineElement::Char(c) = el {
                                let bbox = to_top_left_bbox(c.x0(), c.y0(), c.x1(), c.y1(), geom);
                                let bbox = if let Some(crop) = crop_bbox {
                                    let Some(bbox) = bbox_overlap(bbox, crop) else {
                                        continue;
                                    };
                                    bbox
                                } else {
                                    bbox
                                };
                                let text = c.get_text().to_string();
                                let size = c.size();
                                let upright = c.upright();
                                let width = bbox.width();
                                let height = bbox.height();
                                let doctop = geom.initial_doctop + bbox.top;
                                chars.push(CharObj {
                                    text,
                                    x0: bbox.x0,
                                    x1: bbox.x1,
                                    top: bbox.top,
                                    bottom: bbox.bottom,
                                    doctop,
                                    width,
                                    height,
                                    size,
                                    upright,
                                });
                            }
                        }
                    }
                }
                TextBoxType::Vertical(b) => {
                    for line in b.iter() {
                        for el in line.iter() {
                            if let TextLineElement::Char(c) = el {
                                let bbox = to_top_left_bbox(c.x0(), c.y0(), c.x1(), c.y1(), geom);
                                let bbox = if let Some(crop) = crop_bbox {
                                    let Some(bbox) = bbox_overlap(bbox, crop) else {
                                        continue;
                                    };
                                    bbox
                                } else {
                                    bbox
                                };
                                let text = c.get_text().to_string();
                                let size = c.size();
                                let upright = c.upright();
                                let width = bbox.width();
                                let height = bbox.height();
                                let doctop = geom.initial_doctop + bbox.top;
                                chars.push(CharObj {
                                    text,
                                    x0: bbox.x0,
                                    x1: bbox.x1,
                                    top: bbox.top,
                                    bottom: bbox.bottom,
                                    doctop,
                                    width,
                                    height,
                                    size,
                                    upright,
                                });
                            }
                        }
                    }
                }
            },
            LTItem::Figure(fig) => {
                for child in fig.iter() {
                    visit_item(child, geom, crop_bbox, chars, edges);
                }
            }
            LTItem::Page(p) => {
                for child in p.iter() {
                    visit_item(child, geom, crop_bbox, chars, edges);
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
        visit_item(item, geom, crop_bbox, &mut chars, &mut edges);
    }

    (chars, edges)
}

struct TableFinder {
    page_bbox: BBox,
    chars: Vec<CharObj>,
    edges: Vec<EdgeObj>,
    settings: TableSettings,
}

impl TableFinder {
    fn new(page: &LTPage, geom: &PageGeometry, settings: TableSettings) -> Self {
        let (chars, edges) = collect_page_objects(page, geom);
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
        }
    }

    fn get_edges(&self) -> Vec<EdgeObj> {
        let settings = &self.settings;

        let v_strat = settings.vertical_strategy.as_str();
        let h_strat = settings.horizontal_strategy.as_str();

        let mut words: Vec<WordObj> = Vec::new();
        if v_strat == "text" || h_strat == "text" {
            words = extract_words(&self.chars, &settings.text_settings);
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

pub fn extract_tables_from_ltpage(
    page: &LTPage,
    geom: &PageGeometry,
    settings: &TableSettings,
) -> Vec<Vec<Vec<Option<String>>>> {
    let finder = TableFinder::new(page, geom, settings.clone());
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
        .map(|t| t.extract(&finder.chars, &settings.text_settings))
        .collect()
}

pub fn extract_table_from_ltpage(
    page: &LTPage,
    geom: &PageGeometry,
    settings: &TableSettings,
) -> Option<Vec<Vec<Option<String>>>> {
    let finder = TableFinder::new(page, geom, settings.clone());
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

    Some(tables[best_idx].extract(&finder.chars, &settings.text_settings))
}

pub fn extract_words_from_ltpage(
    page: &LTPage,
    geom: &PageGeometry,
    settings: TextSettings,
) -> Vec<WordObj> {
    let (chars, _edges) = collect_page_objects(page, geom);
    extract_words(&chars, &settings)
}

pub fn extract_text_from_ltpage(
    page: &LTPage,
    geom: &PageGeometry,
    settings: TextSettings,
) -> String {
    let (chars, _edges) = collect_page_objects(page, geom);
    extract_text(&chars, &settings)
}

#[cfg(test)]
mod table_extraction_tests {
    use super::*;

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

        let chars = vec![
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
    fn table_extraction_text_refs_match() {
        let chars = vec![
            CharObj {
                text: "A".to_string(),
                x0: 0.0,
                x1: 1.0,
                top: 0.0,
                bottom: 1.0,
                doctop: 0.0,
                width: 1.0,
                height: 1.0,
                size: 1.0,
                upright: true,
            },
            CharObj {
                text: "B".to_string(),
                x0: 1.1,
                x1: 2.1,
                top: 0.0,
                bottom: 1.0,
                doctop: 0.0,
                width: 1.0,
                height: 1.0,
                size: 1.0,
                upright: true,
            },
        ];

        let settings = TextSettings::default();
        let direct = extract_text(&chars, &settings);
        let ids = vec![CharId(0), CharId(1)];
        let refs = extract_text_from_char_ids(&chars, &ids, &settings);
        assert_eq!(direct, refs);
    }

    #[test]
    fn table_extraction_cells_to_tables_groups_by_corners() {
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
            BBox {
                x0: 20.0,
                top: 0.0,
                x1: 25.0,
                bottom: 5.0,
            },
        ];

        let tables = cells_to_tables(cells);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].len(), 2);
    }

    #[test]
    fn table_extraction_cells_to_tables_graph_matches_default() {
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
            BBox {
                x0: 20.0,
                top: 0.0,
                x1: 25.0,
                bottom: 5.0,
            },
        ];

        let mut expected = cells_to_tables(cells.clone());
        let mut actual = super::cells_to_tables_graph(cells);
        expected.sort_by(|a, b| a.len().cmp(&b.len()));
        actual.sort_by(|a, b| a.len().cmp(&b.len()));
        assert_eq!(expected, actual);
    }
}
