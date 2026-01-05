//! Table extraction types and settings.

use ordered_float::OrderedFloat;

use crate::utils::{Point, Rect};

// Default constants
pub(crate) const DEFAULT_SNAP_TOLERANCE: f64 = 3.0;
pub(crate) const DEFAULT_JOIN_TOLERANCE: f64 = 3.0;
pub(crate) const DEFAULT_MIN_WORDS_VERTICAL: usize = 3;
pub(crate) const DEFAULT_MIN_WORDS_HORIZONTAL: usize = 1;

pub(crate) const DEFAULT_X_TOLERANCE: f64 = 3.0;
pub(crate) const DEFAULT_Y_TOLERANCE: f64 = 3.0;

// Key types for ordered float maps
pub(crate) type KeyF64 = OrderedFloat<f64>;
pub(crate) type KeyPoint = (KeyF64, KeyF64);

pub(crate) fn key_f64(v: f64) -> KeyF64 {
    OrderedFloat(v)
}

pub(crate) fn key_point(x: f64, y: f64) -> KeyPoint {
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
    pub fn from_str(s: &str) -> Option<Self> {
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
    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }
    pub fn height(&self) -> f64 {
        self.bottom - self.top
    }
}

#[derive(Clone, Debug)]
pub struct CharObj {
    pub text: String,
    pub x0: f64,
    pub x1: f64,
    pub top: f64,
    pub bottom: f64,
    pub doctop: f64,
    pub width: f64,
    pub height: f64,
    pub size: f64,
    pub upright: bool,
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

// Internal ID types for efficient indexing
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub(crate) struct VEdgeId(pub usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub(crate) struct HEdgeId(pub usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub(crate) struct CharId(pub usize);

// BBox key for hashing
#[derive(Hash, Eq, PartialEq, Clone, Copy, Debug)]
pub(crate) struct BBoxKey(pub u64, pub u64, pub u64, pub u64);

pub(crate) fn bbox_key(b: &BBox) -> BBoxKey {
    BBoxKey(
        b.x0.to_bits(),
        b.top.to_bits(),
        b.x1.to_bits(),
        b.bottom.to_bits(),
    )
}
