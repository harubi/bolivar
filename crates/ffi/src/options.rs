//! Document and table options as the JVM passes them. Parsing converts types
//! and reads presence bits; every rule about values lives in bolivar-core
//! (`LAParams::try_new`, `TableSettingsPatch::apply_to`).

use bolivar_core::extract::ExtractOptions;
use bolivar_core::layout::LAParams;
use bolivar_core::pdfdocument::OpenOptions;
use bolivar_core::table::{ExplicitLine, TableSettings, TableSettingsPatch, TableStrategy};

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

/// Declares presence bits, one per name in order, with [`NAMES`](opt::NAMES) and
/// [`ALL`](opt::ALL) generated from the same list so that the names `bolivar_describe` exports
/// cannot drift from the bits.
macro_rules! bits {
    ($($(#[$doc:meta])* $name:ident),* $(,)?) => {
        #[allow(non_camel_case_types, clippy::upper_case_acronyms, dead_code)]
        #[repr(u32)]
        enum Bit {
            $($name),*
        }
        $($(#[$doc])* pub const $name: u32 = 1 << Bit::$name as u32;)*
        /// Every bit.
        pub const ALL: u32 = $($name)|*;
        /// Every bit, by the name Java gives it.
        pub const NAMES: &[(&str, u32)] = &[$((stringify!($name), $name)),*];
    };
}

/// Presence bits of [`Options`], exported to Java by name.
pub mod opt {
    bits! {
        PASSWORD,
        PAGES,
        MAX_PAGES,
        CACHING,
        BIDI,
        LINE_OVERLAP,
        CHAR_MARGIN,
        LINE_MARGIN,
        WORD_MARGIN,
        BOXES_FLOW,
        /// `boxes_flow` set to none: disable the boxes-flow ordering.
        BOXES_FLOW_NONE,
        DETECT_VERTICAL,
        ALL_TEXTS,
    }
    pub const LAYOUT: u32 = LINE_OVERLAP
        | CHAR_MARGIN
        | LINE_MARGIN
        | WORD_MARGIN
        | BOXES_FLOW
        | BOXES_FLOW_NONE
        | DETECT_VERTICAL
        | ALL_TEXTS;
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

/// Presence bits of [`TableOptions`], exported to Java by name.
pub mod topt {
    bits! {
        VERTICAL_STRATEGY,
        HORIZONTAL_STRATEGY,
        SNAP,
        SNAP_X,
        SNAP_Y,
        JOIN,
        JOIN_X,
        JOIN_Y,
        INTERSECTION,
        INTERSECTION_X,
        INTERSECTION_Y,
        EXPLICIT_VERTICAL,
        EXPLICIT_HORIZONTAL,
        CROP,
        FIRST_PAGE_CROP,
        MAX_PAGES,
    }
}

/// Table strategy codes, exported to Java by name.
pub const STRATEGIES: &[(&str, TableStrategy)] = &[
    ("LINES", TableStrategy::Lines),
    ("LINES_STRICT", TableStrategy::LinesStrict),
    ("TEXT", TableStrategy::Text),
    ("EXPLICIT", TableStrategy::Explicit),
];

fn check_header(
    struct_size: u32,
    expected: usize,
    present: u32,
    all: u32,
    field: &'static str,
) -> FfiResult<()> {
    if (struct_size as usize) < expected {
        return Err(FfiError::invalid(
            field,
            format!("struct_size {struct_size} is smaller than {expected}"),
        ));
    }
    if present & !all != 0 {
        return Err(FfiError::invalid(
            field,
            format!("unknown option bits {:#x}", present & !all),
        ));
    }
    Ok(())
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

/// Parsed document options: how to open, and how to extract.
#[derive(Debug, Clone)]
pub struct DocumentOptions {
    pub open: OpenOptions,
    pub extract: ExtractOptions,
}

impl DocumentOptions {
    /// Convert raw options. A null pointer means all defaults.
    ///
    /// # Safety
    /// `options` must be null or point to a readable `Options` whose pointers
    /// cover their lengths.
    pub unsafe fn from_raw(options: *const Options) -> FfiResult<Self> {
        let mut open = OpenOptions::default();
        let mut extract = ExtractOptions::default();
        let Some(raw) = (unsafe { options.as_ref() }) else {
            return Ok(Self { open, extract });
        };
        check_header(
            raw.struct_size,
            size_of::<Options>(),
            raw.present,
            opt::ALL,
            "options",
        )?;
        let has = |bit: u32| raw.present & bit != 0;

        if has(opt::PASSWORD) {
            let bytes = unsafe { slice(raw.password, raw.password_len, "password")? };
            open.password = std::str::from_utf8(bytes)
                .map_err(|_| FfiError::invalid("password", "password must be UTF-8"))?
                .to_owned();
        }
        if has(opt::CACHING) {
            open.caching = flag(raw.caching, "caching")?;
            extract.caching = open.caching;
        }
        if has(opt::PAGES) {
            let pages = unsafe { slice(raw.pages, raw.pages_len, "pages")? };
            extract.page_numbers = Some(
                pages
                    .iter()
                    .map(|&page| {
                        positive_count(page, "pages")
                            .map(|page| page - 1)
                            .map_err(|_| {
                                FfiError::invalid(
                                    "pages",
                                    "page numbers are 1-based and must be positive",
                                )
                            })
                    })
                    .collect::<FfiResult<_>>()?,
            );
        }
        if has(opt::MAX_PAGES) {
            extract.maxpages = positive_count(raw.max_pages, "max_pages")?;
        }
        if has(opt::BIDI) {
            extract.bidi = flag(raw.bidi, "bidi")?;
        }
        if raw.present & opt::LAYOUT != 0 {
            if has(opt::BOXES_FLOW) && has(opt::BOXES_FLOW_NONE) {
                return Err(FfiError::invalid(
                    "boxes_flow",
                    "boxes_flow is both set and none",
                ));
            }
            let defaults = LAParams::default();
            let pick = |bit: u32, value: f64, default: f64| if has(bit) { value } else { default };
            let boxes_flow = if has(opt::BOXES_FLOW_NONE) {
                None
            } else if has(opt::BOXES_FLOW) {
                Some(raw.boxes_flow)
            } else {
                defaults.boxes_flow
            };
            extract.laparams = Some(LAParams::try_new(
                pick(opt::LINE_OVERLAP, raw.line_overlap, defaults.line_overlap),
                pick(opt::CHAR_MARGIN, raw.char_margin, defaults.char_margin),
                pick(opt::LINE_MARGIN, raw.line_margin, defaults.line_margin),
                pick(opt::WORD_MARGIN, raw.word_margin, defaults.word_margin),
                boxes_flow,
                if has(opt::DETECT_VERTICAL) {
                    flag(raw.detect_vertical, "detect_vertical")?
                } else {
                    defaults.detect_vertical
                },
                if has(opt::ALL_TEXTS) {
                    flag(raw.all_texts, "all_texts")?
                } else {
                    defaults.all_texts
                },
            )?);
        }
        Ok(Self { open, extract })
    }
}

/// Page crops in pdfplumber space.
#[derive(Debug, Clone, Copy, Default)]
pub struct Crops {
    pub crop: Option<(f64, f64, f64, f64)>,
    pub first_page_crop: Option<(f64, f64, f64, f64)>,
}

impl Crops {
    /// This page's crop: `first_page_crop` wins on page one.
    pub fn for_page(&self, page_index: usize) -> Option<(f64, f64, f64, f64)> {
        if page_index == 0 {
            self.first_page_crop.or(self.crop)
        } else {
            self.crop
        }
    }
}

/// Parsed table options.
#[derive(Debug, Clone, Default)]
pub struct Tables {
    pub settings: TableSettings,
    pub crops: Crops,
    pub max_pages: Option<usize>,
}

fn strategy(code: i32, field: &'static str) -> FfiResult<TableStrategy> {
    usize::try_from(code)
        .ok()
        .and_then(|index| STRATEGIES.get(index))
        .map(|(_, strategy)| *strategy)
        .ok_or_else(|| {
            FfiError::invalid(
                field,
                format!("{field} must be one of lines, lines_strict, text, explicit"),
            )
        })
}

impl Tables {
    /// Convert raw table options. A null pointer means all defaults.
    ///
    /// # Safety
    /// `options` must be null or point to a readable `TableOptions` whose
    /// pointers cover their lengths.
    pub unsafe fn from_raw(options: *const TableOptions) -> FfiResult<Self> {
        let mut tables = Self::default();
        let Some(raw) = (unsafe { options.as_ref() }) else {
            return Ok(tables);
        };
        check_header(
            raw.struct_size,
            size_of::<TableOptions>(),
            raw.present,
            topt::ALL,
            "table_options",
        )?;
        let has = |bit: u32| raw.present & bit != 0;
        let value = |bit: u32, value: f64| has(bit).then_some(value);
        let lines = |bit: u32,
                     ptr: *const f64,
                     len: i64,
                     field: &'static str|
         -> FfiResult<Option<Vec<ExplicitLine>>> {
            if !has(bit) {
                return Ok(None);
            }
            Ok(Some(
                unsafe { slice(ptr, len, field)? }
                    .iter()
                    .map(|&c| ExplicitLine::Coord(c))
                    .collect(),
            ))
        };
        TableSettingsPatch {
            vertical_strategy: has(topt::VERTICAL_STRATEGY)
                .then(|| strategy(raw.vertical_strategy, "vertical_strategy"))
                .transpose()?,
            horizontal_strategy: has(topt::HORIZONTAL_STRATEGY)
                .then(|| strategy(raw.horizontal_strategy, "horizontal_strategy"))
                .transpose()?,
            snap_tolerance: value(topt::SNAP, raw.snap),
            snap_x_tolerance: value(topt::SNAP_X, raw.snap_x),
            snap_y_tolerance: value(topt::SNAP_Y, raw.snap_y),
            join_tolerance: value(topt::JOIN, raw.join),
            join_x_tolerance: value(topt::JOIN_X, raw.join_x),
            join_y_tolerance: value(topt::JOIN_Y, raw.join_y),
            intersection_tolerance: value(topt::INTERSECTION, raw.intersection),
            intersection_x_tolerance: value(topt::INTERSECTION_X, raw.intersection_x),
            intersection_y_tolerance: value(topt::INTERSECTION_Y, raw.intersection_y),
            explicit_vertical_lines: lines(
                topt::EXPLICIT_VERTICAL,
                raw.explicit_vertical,
                raw.explicit_vertical_len,
                "explicit_vertical_lines",
            )?,
            explicit_horizontal_lines: lines(
                topt::EXPLICIT_HORIZONTAL,
                raw.explicit_horizontal,
                raw.explicit_horizontal_len,
                "explicit_horizontal_lines",
            )?,
            ..TableSettingsPatch::default()
        }
        .apply_to_finite(&mut tables.settings)?;
        // Crops are not validated: a degenerate or non-finite crop falls back
        // to the whole page, as pdfplumber's `_sanitize_bbox` does.
        tables.crops = Crops {
            crop: has(topt::CROP).then(|| raw.crop.into()),
            first_page_crop: has(topt::FIRST_PAGE_CROP).then(|| raw.first_page_crop.into()),
        };
        if has(topt::MAX_PAGES) {
            tables.max_pages = Some(positive_count(raw.max_pages, "max_pages")?);
        }
        Ok(tables)
    }
}

impl Options {
    /// All fields absent; for tests and callers that fill a few.
    pub fn empty() -> Self {
        Self {
            struct_size: size_of::<Self>() as u32,
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
}

impl TableOptions {
    /// All fields absent; for tests and callers that fill a few.
    pub fn empty() -> Self {
        Self {
            struct_size: size_of::<Self>() as u32,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field<T: std::fmt::Debug>(result: FfiResult<T>) -> Option<&'static str> {
        result.unwrap_err().field
    }

    #[test]
    fn null_and_empty_options_are_the_defaults() {
        let defaults = unsafe { DocumentOptions::from_raw(std::ptr::null()) }.unwrap();
        let empty = unsafe { DocumentOptions::from_raw(&Options::empty()) }.unwrap();
        assert!(defaults.open.caching);
        assert_eq!(empty.extract.maxpages, 0);
        assert!(empty.extract.laparams.is_none());
        assert!(!empty.extract.bidi);
    }

    #[test]
    fn pages_become_zero_based_and_reject_zero() {
        let pages = [2_i64, 4];
        let mut raw = Options::empty();
        raw.present = opt::PAGES;
        raw.pages = pages.as_ptr();
        raw.pages_len = 2;
        let parsed = unsafe { DocumentOptions::from_raw(&raw) }.unwrap();
        assert_eq!(parsed.extract.page_numbers, Some(vec![1, 3]));
        let zero = [0_i64];
        raw.pages = zero.as_ptr();
        raw.pages_len = 1;
        assert_eq!(
            field(unsafe { DocumentOptions::from_raw(&raw) }),
            Some("pages")
        );
    }

    #[test]
    fn max_pages_must_be_positive() {
        let mut raw = Options::empty();
        raw.present = opt::MAX_PAGES;
        assert_eq!(
            field(unsafe { DocumentOptions::from_raw(&raw) }),
            Some("max_pages")
        );
        raw.max_pages = 3;
        assert_eq!(
            unsafe { DocumentOptions::from_raw(&raw) }
                .unwrap()
                .extract
                .maxpages,
            3
        );
    }

    #[test]
    fn layout_fills_defaults_and_core_validates() {
        let mut raw = Options::empty();
        raw.present = opt::CHAR_MARGIN | opt::BOXES_FLOW_NONE;
        raw.char_margin = 4.0;
        let params = unsafe { DocumentOptions::from_raw(&raw) }
            .unwrap()
            .extract
            .laparams
            .unwrap();
        assert_eq!(params.char_margin, 4.0);
        assert_eq!(params.line_margin, LAParams::default().line_margin);
        assert_eq!(params.boxes_flow, None);

        raw.present = opt::BOXES_FLOW;
        raw.boxes_flow = 1.2;
        assert_eq!(
            field(unsafe { DocumentOptions::from_raw(&raw) }),
            Some("boxes_flow")
        );
        raw.present = opt::LINE_MARGIN;
        raw.line_margin = f64::NAN;
        assert_eq!(
            field(unsafe { DocumentOptions::from_raw(&raw) }),
            Some("line_margin")
        );
    }

    #[test]
    fn bad_bytes_sizes_and_bits_are_rejected() {
        let mut raw = Options::empty();
        raw.present = opt::BIDI;
        raw.bidi = 7;
        assert_eq!(
            field(unsafe { DocumentOptions::from_raw(&raw) }),
            Some("bidi")
        );
        let mut raw = Options::empty();
        raw.struct_size = 8;
        assert_eq!(
            field(unsafe { DocumentOptions::from_raw(&raw) }),
            Some("options")
        );
        let mut raw = Options::empty();
        raw.present = 1 << 30;
        assert_eq!(
            field(unsafe { DocumentOptions::from_raw(&raw) }),
            Some("options")
        );
        let bad = [0xff_u8];
        let mut raw = Options::empty();
        raw.present = opt::PASSWORD;
        raw.password = bad.as_ptr();
        raw.password_len = 1;
        assert_eq!(
            field(unsafe { DocumentOptions::from_raw(&raw) }),
            Some("password")
        );
    }

    #[test]
    fn table_options_resolve_through_core() {
        let mut raw = TableOptions::empty();
        raw.present = topt::SNAP | topt::SNAP_Y;
        raw.snap = 5.0;
        raw.snap_y = 2.0;
        let tables = unsafe { Tables::from_raw(&raw) }.unwrap();
        assert_eq!(tables.settings.snap_x_tolerance, 5.0);
        assert_eq!(tables.settings.snap_y_tolerance, 2.0);

        let mut raw = TableOptions::empty();
        raw.present = topt::JOIN_X;
        raw.join_x = -1.0;
        assert_eq!(
            field(unsafe { Tables::from_raw(&raw) }),
            Some("join_x_tolerance")
        );
        let mut raw = TableOptions::empty();
        raw.present = topt::VERTICAL_STRATEGY;
        raw.vertical_strategy = 9;
        assert_eq!(
            field(unsafe { Tables::from_raw(&raw) }),
            Some("vertical_strategy")
        );
        let mut raw = TableOptions::empty();
        raw.present = topt::MAX_PAGES;
        assert_eq!(field(unsafe { Tables::from_raw(&raw) }), Some("max_pages"));
    }

    #[test]
    fn crops_pass_through_unvalidated() {
        let mut raw = TableOptions::empty();
        raw.present = topt::CROP;
        raw.crop = [0.0, 1.0, 2.0, f64::NAN];
        let tables = unsafe { Tables::from_raw(&raw) }.unwrap();
        assert!(tables.crops.crop.unwrap().3.is_nan());
        assert!(
            tables.crops.for_page(0).unwrap().3.is_nan(),
            "page one falls back to the crop"
        );
        let crops = Crops {
            crop: Some((0.0, 0.0, 1.0, 1.0)),
            first_page_crop: Some((0.0, 0.0, 2.0, 2.0)),
        };
        assert_eq!(crops.for_page(0), crops.first_page_crop);
        assert_eq!(crops.for_page(1), crops.crop);
    }
}
