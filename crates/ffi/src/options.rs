//! Document and table options as the JVM passes them, and the one place
//! they are validated.

use bolivar_core::extract::ExtractOptions as CoreOptions;
use bolivar_core::layout::LAParams;
use bolivar_core::table::{ExplicitLine, TableSettings, TableStrategy};

use crate::abi::{FfiError, FfiResult, flag, slice};

/// Document options. `present` holds one bit per field the caller set.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub struct_size: u32,
    pub present: u32,
    pub caching: u8,
    pub bidi: u8,
    pub detect_vertical: u8,
    pub all_texts: u8,
    pub _pad: u32,
    pub max_pages: i64,
    pub line_overlap: f64,
    pub char_margin: f64,
    pub line_margin: f64,
    pub word_margin: f64,
    pub boxes_flow: f64,
    pub password: *const u8,
    pub password_len: i64,
    pub pages: *const i64,
    pub pages_len: i64,
}

pub mod opt {
    pub const PASSWORD: u32 = 1 << 0;
    pub const PAGES: u32 = 1 << 1;
    pub const MAX_PAGES: u32 = 1 << 2;
    pub const CACHING: u32 = 1 << 3;
    pub const BIDI: u32 = 1 << 4;
    pub const LINE_OVERLAP: u32 = 1 << 5;
    pub const CHAR_MARGIN: u32 = 1 << 6;
    pub const LINE_MARGIN: u32 = 1 << 7;
    pub const WORD_MARGIN: u32 = 1 << 8;
    pub const BOXES_FLOW: u32 = 1 << 9;
    /// `boxes_flow` set to none: disable the boxes-flow ordering.
    pub const BOXES_FLOW_NONE: u32 = 1 << 10;
    pub const DETECT_VERTICAL: u32 = 1 << 11;
    pub const ALL_TEXTS: u32 = 1 << 12;
    pub const LAYOUT: u32 = LINE_OVERLAP
        | CHAR_MARGIN
        | LINE_MARGIN
        | WORD_MARGIN
        | BOXES_FLOW
        | BOXES_FLOW_NONE
        | DETECT_VERTICAL
        | ALL_TEXTS;
    pub const ALL: u32 = (1 << 13) - 1;
}

/// Table options. `present` holds one bit per field the caller set.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TableOptions {
    pub struct_size: u32,
    pub present: u32,
    pub vertical_strategy: i32,
    pub horizontal_strategy: i32,
    pub snap: f64,
    pub snap_x: f64,
    pub snap_y: f64,
    pub join: f64,
    pub join_x: f64,
    pub join_y: f64,
    pub intersection: f64,
    pub intersection_x: f64,
    pub intersection_y: f64,
    pub crop: [f64; 4],
    pub first_page_crop: [f64; 4],
    pub max_pages: i64,
    pub explicit_vertical: *const f64,
    pub explicit_vertical_len: i64,
    pub explicit_horizontal: *const f64,
    pub explicit_horizontal_len: i64,
}

pub mod topt {
    pub const VERTICAL_STRATEGY: u32 = 1 << 0;
    pub const HORIZONTAL_STRATEGY: u32 = 1 << 1;
    pub const SNAP: u32 = 1 << 2;
    pub const SNAP_X: u32 = 1 << 3;
    pub const SNAP_Y: u32 = 1 << 4;
    pub const JOIN: u32 = 1 << 5;
    pub const JOIN_X: u32 = 1 << 6;
    pub const JOIN_Y: u32 = 1 << 7;
    pub const INTERSECTION: u32 = 1 << 8;
    pub const INTERSECTION_X: u32 = 1 << 9;
    pub const INTERSECTION_Y: u32 = 1 << 10;
    pub const EXPLICIT_VERTICAL: u32 = 1 << 11;
    pub const EXPLICIT_HORIZONTAL: u32 = 1 << 12;
    pub const CROP: u32 = 1 << 13;
    pub const FIRST_PAGE_CROP: u32 = 1 << 14;
    pub const MAX_PAGES: u32 = 1 << 15;
    pub const ALL: u32 = (1 << 16) - 1;
}

fn check_size(struct_size: u32, expected: usize, field: &'static str) -> FfiResult<()> {
    if (struct_size as usize) < expected {
        return Err(FfiError::invalid(
            field,
            format!("struct_size {struct_size} is smaller than {expected}"),
        ));
    }
    Ok(())
}

fn check_bits(present: u32, all: u32, field: &'static str) -> FfiResult<()> {
    if present & !all != 0 {
        return Err(FfiError::invalid(
            field,
            format!("unknown option bits {:#x}", present & !all),
        ));
    }
    Ok(())
}

fn finite(value: f64, field: &'static str) -> FfiResult<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(FfiError::invalid(
            field,
            format!("{field} must be a finite number"),
        ))
    }
}

fn positive_count(value: i64, field: &'static str) -> FfiResult<usize> {
    if value < 1 || value > i64::from(u32::MAX) {
        return Err(FfiError::invalid(
            field,
            format!("{field} must be a positive integer"),
        ));
    }
    Ok(value as usize)
}

/// Core options plus what the binding keeps beside them.
#[derive(Debug, Clone)]
pub struct DocOptions {
    pub core: CoreOptions,
}

impl Default for DocOptions {
    fn default() -> Self {
        Self {
            core: CoreOptions {
                password: String::new(),
                page_numbers: None,
                maxpages: 0,
                caching: true,
                laparams: None,
                rotation: 0,
                bidi: false,
            },
        }
    }
}

impl DocOptions {
    /// Validate and convert. A null pointer means all defaults.
    ///
    /// # Safety
    /// `options` must be null or point to a readable `Options` whose pointers
    /// cover their lengths.
    pub unsafe fn from_raw(options: *const Options) -> FfiResult<Self> {
        let Some(raw) = (unsafe { options.as_ref() }) else {
            return Ok(Self::default());
        };
        check_size(raw.struct_size, size_of::<Options>(), "options")?;
        check_bits(raw.present, opt::ALL, "options")?;
        let has = |bit: u32| raw.present & bit != 0;
        let mut result = Self::default();

        if has(opt::PASSWORD) {
            let bytes = unsafe { slice(raw.password, raw.password_len, "password")? };
            result.core.password = std::str::from_utf8(bytes)
                .map_err(|_| FfiError::invalid("password", "password must be UTF-8"))?
                .to_owned();
        }
        if has(opt::PAGES) {
            let pages = unsafe { slice(raw.pages, raw.pages_len, "pages")? };
            result.core.page_numbers = Some(
                pages
                    .iter()
                    .map(|&page| {
                        positive_count(page, "pages").map_err(|_| {
                            FfiError::invalid(
                                "pages",
                                "page numbers are 1-based and must be positive",
                            )
                        })
                    })
                    .map(|page| page.map(|page| page - 1))
                    .collect::<FfiResult<_>>()?,
            );
        }
        if has(opt::MAX_PAGES) {
            result.core.maxpages = positive_count(raw.max_pages, "max_pages")?;
        }
        if has(opt::CACHING) {
            result.core.caching = flag(raw.caching, "caching")?;
        }
        if has(opt::BIDI) {
            result.core.bidi = flag(raw.bidi, "bidi")?;
        }
        if raw.present & opt::LAYOUT != 0 {
            let mut params = LAParams::default();
            if has(opt::LINE_OVERLAP) {
                params.line_overlap = finite(raw.line_overlap, "line_overlap")?;
            }
            if has(opt::CHAR_MARGIN) {
                params.char_margin = finite(raw.char_margin, "char_margin")?;
            }
            if has(opt::LINE_MARGIN) {
                params.line_margin = finite(raw.line_margin, "line_margin")?;
            }
            if has(opt::WORD_MARGIN) {
                params.word_margin = finite(raw.word_margin, "word_margin")?;
            }
            if has(opt::BOXES_FLOW) && has(opt::BOXES_FLOW_NONE) {
                return Err(FfiError::invalid(
                    "boxes_flow",
                    "boxes_flow is both set and none",
                ));
            }
            if has(opt::BOXES_FLOW_NONE) {
                params.boxes_flow = None;
            }
            if has(opt::BOXES_FLOW) {
                let flow = finite(raw.boxes_flow, "boxes_flow")?;
                if !(-1.0..=1.0).contains(&flow) {
                    return Err(FfiError::invalid(
                        "boxes_flow",
                        "boxes_flow must be none or within [-1.0, 1.0]",
                    ));
                }
                params.boxes_flow = Some(flow);
            }
            if has(opt::DETECT_VERTICAL) {
                params.detect_vertical = flag(raw.detect_vertical, "detect_vertical")?;
            }
            if has(opt::ALL_TEXTS) {
                params.all_texts = flag(raw.all_texts, "all_texts")?;
            }
            result.core.laparams = Some(params);
        }
        Ok(result)
    }
}

/// Validated table options.
#[derive(Debug, Clone, Default)]
pub struct Tables {
    pub settings: TableSettings,
    pub crop: Option<(f64, f64, f64, f64)>,
    pub first_page_crop: Option<(f64, f64, f64, f64)>,
    pub max_pages: Option<usize>,
}

fn strategy(code: i32, field: &'static str) -> FfiResult<TableStrategy> {
    match code {
        0 => Ok(TableStrategy::Lines),
        1 => Ok(TableStrategy::LinesStrict),
        2 => Ok(TableStrategy::Text),
        3 => Ok(TableStrategy::Explicit),
        _ => Err(FfiError::invalid(
            field,
            format!("{field} must be one of lines, lines_strict, text, explicit"),
        )),
    }
}

fn tolerance(value: f64, field: &'static str) -> FfiResult<f64> {
    if !value.is_finite() || value < 0.0 {
        return Err(FfiError::invalid(
            field,
            format!("{field} must be a finite number >= 0"),
        ));
    }
    Ok(value)
}

fn crop(values: [f64; 4]) -> (f64, f64, f64, f64) {
    (values[0], values[1], values[2], values[3])
}

impl Tables {
    /// Validate and convert. A null pointer means all defaults.
    ///
    /// # Safety
    /// `options` must be null or point to a readable `TableOptions` whose
    /// pointers cover their lengths.
    pub unsafe fn from_raw(options: *const TableOptions) -> FfiResult<Self> {
        let Some(raw) = (unsafe { options.as_ref() }) else {
            return Ok(Self::default());
        };
        check_size(raw.struct_size, size_of::<TableOptions>(), "table_options")?;
        check_bits(raw.present, topt::ALL, "table_options")?;
        let has = |bit: u32| raw.present & bit != 0;
        let mut s = TableSettings::default();

        if has(topt::VERTICAL_STRATEGY) {
            s.vertical_strategy = strategy(raw.vertical_strategy, "vertical_strategy")?;
        }
        if has(topt::HORIZONTAL_STRATEGY) {
            s.horizontal_strategy = strategy(raw.horizontal_strategy, "horizontal_strategy")?;
        }
        // General tolerances fan into both axes; axis-specific values override.
        if has(topt::SNAP) {
            let v = tolerance(raw.snap, "snap_tolerance")?;
            (s.snap_x_tolerance, s.snap_y_tolerance) = (v, v);
        }
        if has(topt::SNAP_X) {
            s.snap_x_tolerance = tolerance(raw.snap_x, "snap_x_tolerance")?;
        }
        if has(topt::SNAP_Y) {
            s.snap_y_tolerance = tolerance(raw.snap_y, "snap_y_tolerance")?;
        }
        if has(topt::JOIN) {
            let v = tolerance(raw.join, "join_tolerance")?;
            (s.join_x_tolerance, s.join_y_tolerance) = (v, v);
        }
        if has(topt::JOIN_X) {
            s.join_x_tolerance = tolerance(raw.join_x, "join_x_tolerance")?;
        }
        if has(topt::JOIN_Y) {
            s.join_y_tolerance = tolerance(raw.join_y, "join_y_tolerance")?;
        }
        if has(topt::INTERSECTION) {
            let v = tolerance(raw.intersection, "intersection_tolerance")?;
            (s.intersection_x_tolerance, s.intersection_y_tolerance) = (v, v);
        }
        if has(topt::INTERSECTION_X) {
            s.intersection_x_tolerance = tolerance(raw.intersection_x, "intersection_x_tolerance")?;
        }
        if has(topt::INTERSECTION_Y) {
            s.intersection_y_tolerance = tolerance(raw.intersection_y, "intersection_y_tolerance")?;
        }
        let lines = |ptr, len, field| -> FfiResult<Vec<ExplicitLine>> {
            unsafe { slice(ptr, len, field)? }
                .iter()
                .map(|&coord| finite(coord, field).map(ExplicitLine::Coord))
                .collect()
        };
        if has(topt::EXPLICIT_VERTICAL) {
            s.explicit_vertical_lines = lines(
                raw.explicit_vertical,
                raw.explicit_vertical_len,
                "explicit_vertical_lines",
            )?;
        }
        if has(topt::EXPLICIT_HORIZONTAL) {
            s.explicit_horizontal_lines = lines(
                raw.explicit_horizontal,
                raw.explicit_horizontal_len,
                "explicit_horizontal_lines",
            )?;
        }
        Ok(Self {
            settings: s,
            // Crops are not validated: a degenerate or non-finite crop falls
            // back to the whole page, as pdfplumber's `_sanitize_bbox` does.
            crop: has(topt::CROP).then(|| crop(raw.crop)),
            first_page_crop: has(topt::FIRST_PAGE_CROP).then(|| crop(raw.first_page_crop)),
            max_pages: if has(topt::MAX_PAGES) {
                Some(positive_count(raw.max_pages, "max_pages")?)
            } else {
                None
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> Options {
        Options {
            struct_size: size_of::<Options>() as u32,
            present: 0,
            caching: 0,
            bidi: 0,
            detect_vertical: 0,
            all_texts: 0,
            _pad: 0,
            max_pages: 0,
            line_overlap: 0.0,
            char_margin: 0.0,
            line_margin: 0.0,
            word_margin: 0.0,
            boxes_flow: 0.0,
            password: std::ptr::null(),
            password_len: 0,
            pages: std::ptr::null(),
            pages_len: 0,
        }
    }

    fn table_options() -> TableOptions {
        TableOptions {
            struct_size: size_of::<TableOptions>() as u32,
            present: 0,
            vertical_strategy: 0,
            horizontal_strategy: 0,
            snap: 0.0,
            snap_x: 0.0,
            snap_y: 0.0,
            join: 0.0,
            join_x: 0.0,
            join_y: 0.0,
            intersection: 0.0,
            intersection_x: 0.0,
            intersection_y: 0.0,
            crop: [0.0; 4],
            first_page_crop: [0.0; 4],
            max_pages: 0,
            explicit_vertical: std::ptr::null(),
            explicit_vertical_len: 0,
            explicit_horizontal: std::ptr::null(),
            explicit_horizontal_len: 0,
        }
    }

    fn field(result: FfiResult<impl std::fmt::Debug>) -> Option<&'static str> {
        result.unwrap_err().field
    }

    #[test]
    fn null_and_empty_options_are_the_defaults() {
        let defaults = unsafe { DocOptions::from_raw(std::ptr::null()) }.unwrap();
        let empty = unsafe { DocOptions::from_raw(&options()) }.unwrap();
        assert!(defaults.core.caching);
        assert_eq!(empty.core.maxpages, 0);
        assert!(empty.core.laparams.is_none());
        assert!(!empty.core.bidi);
    }

    #[test]
    fn pages_become_zero_based_and_reject_zero() {
        let pages = [2_i64, 4];
        let mut raw = options();
        raw.present = opt::PAGES;
        raw.pages = pages.as_ptr();
        raw.pages_len = 2;
        let parsed = unsafe { DocOptions::from_raw(&raw) }.unwrap();
        assert_eq!(parsed.core.page_numbers, Some(vec![1, 3]));
        let zero = [0_i64];
        raw.pages = zero.as_ptr();
        raw.pages_len = 1;
        assert_eq!(field(unsafe { DocOptions::from_raw(&raw) }), Some("pages"));
    }

    #[test]
    fn max_pages_must_be_positive() {
        let mut raw = options();
        raw.present = opt::MAX_PAGES;
        raw.max_pages = 0;
        assert_eq!(
            field(unsafe { DocOptions::from_raw(&raw) }),
            Some("max_pages")
        );
        raw.max_pages = 3;
        assert_eq!(
            unsafe { DocOptions::from_raw(&raw) }.unwrap().core.maxpages,
            3
        );
    }

    #[test]
    fn layout_fills_defaults_and_validates() {
        let mut raw = options();
        raw.present = opt::CHAR_MARGIN | opt::BOXES_FLOW_NONE;
        raw.char_margin = 4.0;
        let params = unsafe { DocOptions::from_raw(&raw) }
            .unwrap()
            .core
            .laparams
            .unwrap();
        assert_eq!(params.char_margin, 4.0);
        assert_eq!(params.line_margin, LAParams::default().line_margin);
        assert_eq!(params.boxes_flow, None);

        raw.present = opt::BOXES_FLOW;
        raw.boxes_flow = 1.2;
        assert_eq!(
            field(unsafe { DocOptions::from_raw(&raw) }),
            Some("boxes_flow")
        );
        raw.present = opt::LINE_MARGIN;
        raw.line_margin = f64::NAN;
        assert_eq!(
            field(unsafe { DocOptions::from_raw(&raw) }),
            Some("line_margin")
        );
    }

    #[test]
    fn bad_bytes_sizes_and_bits_are_rejected() {
        let mut raw = options();
        raw.present = opt::BIDI;
        raw.bidi = 7;
        assert_eq!(field(unsafe { DocOptions::from_raw(&raw) }), Some("bidi"));
        let mut raw = options();
        raw.struct_size = 8;
        assert_eq!(
            field(unsafe { DocOptions::from_raw(&raw) }),
            Some("options")
        );
        let mut raw = options();
        raw.present = 1 << 30;
        assert_eq!(
            field(unsafe { DocOptions::from_raw(&raw) }),
            Some("options")
        );
        let bad = [0xff_u8];
        let mut raw = options();
        raw.present = opt::PASSWORD;
        raw.password = bad.as_ptr();
        raw.password_len = 1;
        assert_eq!(
            field(unsafe { DocOptions::from_raw(&raw) }),
            Some("password")
        );
    }

    #[test]
    fn tolerances_fan_out_then_axis_values_override() {
        let mut raw = table_options();
        raw.present = topt::SNAP | topt::SNAP_Y;
        raw.snap = 5.0;
        raw.snap_y = 2.0;
        let tables = unsafe { Tables::from_raw(&raw) }.unwrap();
        assert_eq!(tables.settings.snap_x_tolerance, 5.0);
        assert_eq!(tables.settings.snap_y_tolerance, 2.0);
    }

    #[test]
    fn table_options_reject_bad_values() {
        let mut raw = table_options();
        raw.present = topt::JOIN_X;
        raw.join_x = -1.0;
        assert_eq!(
            field(unsafe { Tables::from_raw(&raw) }),
            Some("join_x_tolerance")
        );
        let mut raw = table_options();
        raw.present = topt::VERTICAL_STRATEGY;
        raw.vertical_strategy = 9;
        assert_eq!(
            field(unsafe { Tables::from_raw(&raw) }),
            Some("vertical_strategy")
        );
        let mut raw = table_options();
        raw.present = topt::MAX_PAGES;
        assert_eq!(field(unsafe { Tables::from_raw(&raw) }), Some("max_pages"));
        let lines = [1.0, f64::INFINITY];
        let mut raw = table_options();
        raw.present = topt::EXPLICIT_VERTICAL;
        raw.explicit_vertical = lines.as_ptr();
        raw.explicit_vertical_len = 2;
        assert_eq!(
            field(unsafe { Tables::from_raw(&raw) }),
            Some("explicit_vertical_lines")
        );
    }

    #[test]
    fn crops_pass_through_unvalidated() {
        let mut raw = table_options();
        raw.present = topt::CROP;
        raw.crop = [0.0, 1.0, 2.0, f64::NAN];
        let tables = unsafe { Tables::from_raw(&raw) }.unwrap();
        assert!(tables.crop.unwrap().3.is_nan());
    }
}
