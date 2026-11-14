//! Table extraction types and settings.

use lasso::Spur;
use ordered_float::OrderedFloat;

use crate::error::InvalidSetting;
use crate::utils::{Point, Rect};

// Default constants
pub const DEFAULT_SNAP_TOLERANCE: f64 = 3.0;
pub const DEFAULT_JOIN_TOLERANCE: f64 = 3.0;
pub const DEFAULT_MIN_WORDS_VERTICAL: usize = 3;
pub const DEFAULT_MIN_WORDS_HORIZONTAL: usize = 1;

pub const DEFAULT_X_TOLERANCE: f64 = 3.0;
pub const DEFAULT_Y_TOLERANCE: f64 = 3.0;

// Key types for ordered float maps
pub type KeyF64 = OrderedFloat<f64>;
pub type KeyPoint = (KeyF64, KeyF64);

pub const fn key_f64(v: f64) -> KeyF64 {
    OrderedFloat(v)
}

pub const fn key_point(x: f64, y: f64) -> KeyPoint {
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

impl std::str::FromStr for TextDir {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ttb" => Ok(Self::Ttb),
            "btt" => Ok(Self::Btt),
            "ltr" => Ok(Self::Ltr),
            "rtl" => Ok(Self::Rtl),
            _ => Err(()),
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
    pub text: Spur,
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
    pub probe_policy: TableProbePolicy,
    pub vertical_strategy: TableStrategy,
    pub horizontal_strategy: TableStrategy,
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
            probe_policy: TableProbePolicy::Auto,
            vertical_strategy: TableStrategy::Lines,
            horizontal_strategy: TableStrategy::Lines,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableStrategy {
    Lines,
    LinesStrict,
    Text,
    Explicit,
}

impl TableStrategy {
    pub const fn uses_text(self) -> bool {
        matches!(self, Self::Text)
    }
}

impl std::str::FromStr for TableStrategy {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "lines" => Ok(Self::Lines),
            "lines_strict" => Ok(Self::LinesStrict),
            "text" => Ok(Self::Text),
            "explicit" => Ok(Self::Explicit),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableProbePolicy {
    Auto,
    Always,
    Never,
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
    /// Use ICU to reconstruct visual bidirectional text into logical order.
    pub bidi: bool,
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
            bidi: false,
        }
    }
}

/// Table settings given as optional values, resolved as pdfplumber resolves
/// them: a general tolerance fills both axes, and an axis-specific value
/// always wins over it, whatever order they were given in.
#[derive(Clone, Debug, Default)]
pub struct TableSettingsPatch {
    pub vertical_strategy: Option<TableStrategy>,
    pub horizontal_strategy: Option<TableStrategy>,
    pub snap_tolerance: Option<f64>,
    pub snap_x_tolerance: Option<f64>,
    pub snap_y_tolerance: Option<f64>,
    pub join_tolerance: Option<f64>,
    pub join_x_tolerance: Option<f64>,
    pub join_y_tolerance: Option<f64>,
    pub intersection_tolerance: Option<f64>,
    pub intersection_x_tolerance: Option<f64>,
    pub intersection_y_tolerance: Option<f64>,
    pub edge_min_length: Option<f64>,
    pub edge_min_length_prefilter: Option<f64>,
    /// Signed, so that a negative count fails in pdfplumber's order, not while parsing.
    pub min_words_vertical: Option<i64>,
    pub min_words_horizontal: Option<i64>,
    pub explicit_vertical_lines: Option<Vec<ExplicitLine>>,
    pub explicit_horizontal_lines: Option<Vec<ExplicitLine>>,
}

/// Whether non-finite numbers are refused, beyond pdfplumber's checks.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Finite {
    /// pdfplumber's rules: only negative values fail; infinity and NaN pass.
    Allowed,
    /// Also refuse infinity and NaN in tolerances and explicit lines.
    Required,
}

/// The general value, overridden by the axis value when one is given.
fn axis(general: Option<f64>, specific: Option<f64>, current: f64) -> f64 {
    specific.or(general).unwrap_or(current)
}

impl TableSettingsPatch {
    /// Apply the given values to `settings` and validate the result, by pdfplumber's rules:
    /// every given tolerance is checked, in pdfplumber's order, even one that both axis values
    /// override, and only a negative value fails. On error, `settings` is unchanged.
    pub fn apply_to(self, settings: &mut TableSettings) -> Result<(), InvalidSetting> {
        self.apply(settings, Finite::Allowed)
    }

    /// [`Self::apply_to`], also refusing infinity and NaN in tolerances and explicit lines.
    pub fn apply_to_finite(self, settings: &mut TableSettings) -> Result<(), InvalidSetting> {
        self.apply(settings, Finite::Required)
    }

    /// Check every given value by pdfplumber's rules: its NON_NEGATIVE_SETTINGS, in its
    /// order, naming the first that fails. Strategies are checked after these, by the caller
    /// that parses them, as pdfplumber does.
    pub fn check(&self) -> Result<(), InvalidSetting> {
        self.check_with(Finite::Allowed)
    }

    fn check_with(&self, finite: Finite) -> Result<(), InvalidSetting> {
        let counts = |value: Option<i64>| value.map(|count| count as f64);
        let values = [
            ("snap_tolerance", self.snap_tolerance),
            ("snap_x_tolerance", self.snap_x_tolerance),
            ("snap_y_tolerance", self.snap_y_tolerance),
            ("join_tolerance", self.join_tolerance),
            ("join_x_tolerance", self.join_x_tolerance),
            ("join_y_tolerance", self.join_y_tolerance),
            ("edge_min_length", self.edge_min_length),
            ("edge_min_length_prefilter", self.edge_min_length_prefilter),
            ("min_words_vertical", counts(self.min_words_vertical)),
            ("min_words_horizontal", counts(self.min_words_horizontal)),
            ("intersection_tolerance", self.intersection_tolerance),
            ("intersection_x_tolerance", self.intersection_x_tolerance),
            ("intersection_y_tolerance", self.intersection_y_tolerance),
        ];
        for (field, value) in values {
            let Some(value) = value else { continue };
            if finite == Finite::Required && !value.is_finite() {
                return Err(InvalidSetting::new(
                    field,
                    format!("Table setting '{field}' must be a finite number"),
                ));
            }
            if value < 0.0 {
                return Err(InvalidSetting::new(
                    field,
                    format!("Table setting '{field}' cannot be negative"),
                ));
            }
        }
        if finite == Finite::Required {
            for (field, lines) in [
                ("explicit_vertical_lines", &self.explicit_vertical_lines),
                ("explicit_horizontal_lines", &self.explicit_horizontal_lines),
            ] {
                let non_finite = lines
                    .iter()
                    .flatten()
                    .any(|line| matches!(line, ExplicitLine::Coord(coord) if !coord.is_finite()));
                if non_finite {
                    return Err(InvalidSetting::new(
                        field,
                        format!("{field} must hold finite numbers"),
                    ));
                }
            }
        }
        Ok(())
    }

    fn apply(self, settings: &mut TableSettings, finite: Finite) -> Result<(), InvalidSetting> {
        self.check_with(finite)?;
        let mut next = settings.clone();
        if let Some(strategy) = self.vertical_strategy {
            next.vertical_strategy = strategy;
        }
        if let Some(strategy) = self.horizontal_strategy {
            next.horizontal_strategy = strategy;
        }
        next.snap_x_tolerance = axis(
            self.snap_tolerance,
            self.snap_x_tolerance,
            next.snap_x_tolerance,
        );
        next.snap_y_tolerance = axis(
            self.snap_tolerance,
            self.snap_y_tolerance,
            next.snap_y_tolerance,
        );
        next.join_x_tolerance = axis(
            self.join_tolerance,
            self.join_x_tolerance,
            next.join_x_tolerance,
        );
        next.join_y_tolerance = axis(
            self.join_tolerance,
            self.join_y_tolerance,
            next.join_y_tolerance,
        );
        next.intersection_x_tolerance = axis(
            self.intersection_tolerance,
            self.intersection_x_tolerance,
            next.intersection_x_tolerance,
        );
        next.intersection_y_tolerance = axis(
            self.intersection_tolerance,
            self.intersection_y_tolerance,
            next.intersection_y_tolerance,
        );
        if let Some(value) = self.edge_min_length {
            next.edge_min_length = value;
        }
        if let Some(value) = self.edge_min_length_prefilter {
            next.edge_min_length_prefilter = value;
        }
        // Checked non-negative above.
        if let Some(value) = self.min_words_vertical {
            next.min_words_vertical = value as usize;
        }
        if let Some(value) = self.min_words_horizontal {
            next.min_words_horizontal = value as usize;
        }
        if let Some(lines) = self.explicit_vertical_lines {
            next.explicit_vertical_lines = lines;
        }
        if let Some(lines) = self.explicit_horizontal_lines {
            next.explicit_horizontal_lines = lines;
        }
        next.validate()?;
        *settings = next;
        Ok(())
    }
}

impl TableSettings {
    /// Check the rules that span several settings: an explicit strategy needs
    /// at least two explicit lines on its axis.
    pub fn validate(&self) -> Result<(), InvalidSetting> {
        for (strategy, lines, orientation, field) in [
            (
                self.vertical_strategy,
                &self.explicit_vertical_lines,
                "vertical",
                "explicit_vertical_lines",
            ),
            (
                self.horizontal_strategy,
                &self.explicit_horizontal_lines,
                "horizontal",
                "explicit_horizontal_lines",
            ),
        ] {
            if strategy == TableStrategy::Explicit && lines.len() < 2 {
                return Err(InvalidSetting::new(
                    field,
                    format!(
                        "If {orientation}_strategy == 'explicit', explicit_{orientation}_lines must be specified as a list/tuple of two or more floats/ints."
                    ),
                ));
            }
        }
        Ok(())
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
pub struct VEdgeId(pub u32);

impl VEdgeId {
    pub(super) fn from_index(index: usize) -> Self {
        Self(u32::try_from(index).expect("a page cannot contain more than u32::MAX vertical edges"))
    }

    pub(super) fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct HEdgeId(pub u32);

impl HEdgeId {
    pub(super) fn from_index(index: usize) -> Self {
        Self(
            u32::try_from(index)
                .expect("a page cannot contain more than u32::MAX horizontal edges"),
        )
    }

    pub(super) fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct CharId(pub u32);

impl CharId {
    pub(super) fn from_index(index: usize) -> Self {
        Self(u32::try_from(index).expect("a page cannot contain more than u32::MAX characters"))
    }

    pub(super) fn index(self) -> usize {
        self.0 as usize
    }
}

// BBox key for hashing
#[cfg(test)]
#[derive(Hash, Eq, PartialEq, Clone, Copy, Debug)]
pub struct BBoxKey(pub u64, pub u64, pub u64, pub u64);

#[cfg(test)]
pub const fn bbox_key(b: &BBox) -> BBoxKey {
    BBoxKey(
        b.x0.to_bits(),
        b.top.to_bits(),
        b.x1.to_bits(),
        b.bottom.to_bits(),
    )
}

#[cfg(test)]
mod patch_tests {
    use super::*;

    #[test]
    fn axis_values_win_over_general_ones() {
        let mut settings = TableSettings::default();
        TableSettingsPatch {
            snap_tolerance: Some(5.0),
            snap_x_tolerance: Some(1.0),
            join_tolerance: Some(2.0),
            ..Default::default()
        }
        .apply_to(&mut settings)
        .unwrap();
        assert_eq!(settings.snap_x_tolerance, 1.0);
        assert_eq!(settings.snap_y_tolerance, 5.0);
        assert_eq!(settings.join_x_tolerance, 2.0);
        assert_eq!(settings.join_y_tolerance, 2.0);
        assert_eq!(settings.intersection_x_tolerance, 3.0);
    }

    #[test]
    fn bad_values_name_their_field() {
        let field = |patch: TableSettingsPatch| {
            patch
                .apply_to(&mut TableSettings::default())
                .unwrap_err()
                .field
        };
        assert_eq!(
            field(TableSettingsPatch {
                join_x_tolerance: Some(-1.0),
                ..Default::default()
            }),
            "join_x_tolerance"
        );
        // pdfplumber checks a general tolerance even when both axes override it, and names the
        // first bad value in its own order.
        assert_eq!(
            field(TableSettingsPatch {
                snap_tolerance: Some(-1.0),
                snap_x_tolerance: Some(1.0),
                snap_y_tolerance: Some(1.0),
                ..Default::default()
            }),
            "snap_tolerance"
        );
        assert_eq!(
            field(TableSettingsPatch {
                intersection_y_tolerance: Some(-1.0),
                join_tolerance: Some(-1.0),
                ..Default::default()
            }),
            "join_tolerance"
        );
        assert_eq!(
            field(TableSettingsPatch {
                vertical_strategy: Some(TableStrategy::Explicit),
                explicit_vertical_lines: Some(vec![ExplicitLine::Coord(1.0)]),
                ..Default::default()
            }),
            "explicit_vertical_lines"
        );
    }

    #[test]
    fn non_finite_values_pass_pdfplumbers_rules_but_not_the_finite_ones() {
        let patch = || TableSettingsPatch {
            snap_tolerance: Some(f64::NAN),
            intersection_tolerance: Some(f64::INFINITY),
            explicit_horizontal_lines: Some(vec![
                ExplicitLine::Coord(0.0),
                ExplicitLine::Coord(f64::INFINITY),
            ]),
            ..Default::default()
        };
        let mut settings = TableSettings::default();
        patch()
            .apply_to(&mut settings)
            .expect("pdfplumber accepts infinity and NaN");
        assert!(settings.snap_x_tolerance.is_nan());
        assert_eq!(settings.intersection_y_tolerance, f64::INFINITY);

        let mut settings = TableSettings::default();
        let err = patch().apply_to_finite(&mut settings).unwrap_err();
        assert_eq!(err.field, "snap_tolerance");
        assert_eq!(
            settings.snap_x_tolerance,
            TableSettings::default().snap_x_tolerance,
            "unchanged"
        );
        let lines_only = TableSettingsPatch {
            explicit_horizontal_lines: Some(vec![ExplicitLine::Coord(f64::NAN)]),
            ..Default::default()
        };
        assert_eq!(
            lines_only
                .apply_to_finite(&mut TableSettings::default())
                .unwrap_err()
                .field,
            "explicit_horizontal_lines"
        );
    }

    #[test]
    fn converted_errors_keep_their_field() {
        let error: crate::error::PdfError = TableSettingsPatch {
            snap_tolerance: Some(-1.0),
            ..Default::default()
        }
        .apply_to(&mut TableSettings::default())
        .unwrap_err()
        .into();
        assert_eq!(error.field(), Some("snap_tolerance"));
        assert_eq!(error.kind(), crate::error::ErrorKind::InvalidArgument);
        assert_eq!(
            error.to_string(),
            "invalid argument: Table setting 'snap_tolerance' cannot be negative"
        );
    }
}
