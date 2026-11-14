//! Columnar batches handed to the JVM.
//!
//! A batch is a header and a directory of typed columns. Java reads the
//! header at the batch address, checks each column's element type and length,
//! copies the columns into Java arrays, and frees the batch.
//!
//! Every kind starts with five common columns: `Text` (UTF-16), `Strings` and
//! `StringOff` (an interned string table), `Ints` and `Floats` (named scalar
//! slots). Each kind declares its slots and columns once, in [`schema!`]; the
//! names are exported through `bolivar_schema` so Java can check it agrees.
//!
//! Ranges into `Text` are `i32` start/end pairs; `-1, -1` is absent.

use std::rc::Rc;

use rustc_hash::FxHashMap;

use crate::abi::{Code, ErrorOut, FfiError, FfiResult, free_handle, guard, write_out};

pub const MAGIC: u32 = 0x424F_4C56; // "BOLV"

/// Column element types. Java mirrors these.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Elem {
    F64 = 1,
    I32 = 2,
    I64 = 3,
    U8 = 4,
    U16 = 5,
}

#[repr(C)]
#[derive(Debug)]
pub struct ColumnDesc {
    pub ptr: *const u8,
    pub len: i64,
    pub elem: u32,
    pub _pad: u32,
}

#[repr(C)]
#[derive(Debug)]
pub struct BatchHeader {
    pub magic: u32,
    pub abi_version: u32,
    pub kind: u32,
    pub ncols: u32,
    pub cols: *const ColumnDesc,
}

#[derive(Debug)]
pub enum Column {
    F64(Vec<f64>),
    I32(Vec<i32>),
    I64(Vec<i64>),
    U8(Vec<u8>),
    U16(Vec<u16>),
}

impl Column {
    fn new(elem: Elem) -> Self {
        match elem {
            Elem::F64 => Self::F64(Vec::new()),
            Elem::I32 => Self::I32(Vec::new()),
            Elem::I64 => Self::I64(Vec::new()),
            Elem::U8 => Self::U8(Vec::new()),
            Elem::U16 => Self::U16(Vec::new()),
        }
    }

    fn desc(&self) -> ColumnDesc {
        let (ptr, len, elem) = match self {
            Self::F64(v) => (v.as_ptr().cast(), v.len(), Elem::F64),
            Self::I32(v) => (v.as_ptr().cast(), v.len(), Elem::I32),
            Self::I64(v) => (v.as_ptr().cast(), v.len(), Elem::I64),
            Self::U8(v) => (v.as_ptr(), v.len(), Elem::U8),
            Self::U16(v) => (v.as_ptr().cast(), v.len(), Elem::U16),
        };
        ColumnDesc {
            ptr,
            len: len as i64,
            elem: elem as u32,
            _pad: 0,
        }
    }
}

/// A finished batch's columns. `Send`, so workers build batches in parallel;
/// the consumer turns it into a [`Batch`].
#[derive(Debug)]
pub struct Built {
    kind: u32,
    columns: Vec<Column>,
}

impl Built {
    /// Attach the column directory the JVM reads.
    pub fn into_batch(self) -> Batch {
        let descs: Vec<ColumnDesc> = self.columns.iter().map(Column::desc).collect();
        let header = BatchHeader {
            magic: MAGIC,
            abi_version: crate::ABI_VERSION,
            kind: self.kind,
            ncols: descs.len() as u32,
            cols: descs.as_ptr(),
        };
        Batch {
            header,
            _descs: descs,
            columns: self.columns,
        }
    }

    #[cfg(test)]
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }
}

/// A batch handed to the JVM. The header is the first field, so a batch
/// pointer is a header pointer. The descriptors point into `columns`, whose
/// buffers never move once built.
#[repr(C)]
#[derive(Debug)]
pub struct Batch {
    header: BatchHeader,
    _descs: Vec<ColumnDesc>,
    columns: Vec<Column>,
}

impl Batch {
    #[cfg(test)]
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }
}

/// One kind's layout: its number, scalar slots and columns.
pub struct Kind {
    pub number: u32,
    pub ints: &'static [&'static str],
    pub floats: &'static [&'static str],
    pub ranges: &'static [&'static str],
    pub flags: &'static [&'static str],
    pub columns: &'static [(&'static str, Elem)],
}

/// The five columns every kind starts with.
pub const COMMON: [(&str, Elem); 5] = [
    ("Text", Elem::U16),
    ("Strings", Elem::U16),
    ("StringOff", Elem::I32),
    ("Ints", Elem::I64),
    ("Floats", Elem::F64),
];

/// Indices of the common columns.
pub mod common {
    pub const TEXT: usize = 0;
    pub const STRINGS: usize = 1;
    pub const STRING_OFF: usize = 2;
    pub const INTS: usize = 3;
    pub const FLOATS: usize = 4;
}

/// Declare a batch kind: its number, named scalar slots, named ranges (held
/// in a `Ranges` column), named flag bits, and columns after the common five.
/// Every group may be left out when it is empty.
macro_rules! schema {
    ($module:ident, $kind:expr
     $(, ints: [$($int:ident),* $(,)?])?
     $(, floats: [$($float:ident),* $(,)?])?
     $(, ranges: [$($range:ident),* $(,)?])?
     $(, flags: [$($flag:ident),* $(,)?])?
     $(, columns: [$($col:ident : $elem:ident),* $(,)?])?
     $(,)?) => {
        pub mod $module {
            #![allow(dead_code)]
            use crate::batch::{Elem, Kind, COMMON};

            pub const KIND: u32 = $kind;

            /// Columns, by index.
            #[derive(Debug, Clone, Copy)]
            pub enum Col { Text, Strings, StringOff, Ints, Floats, $($($col),*)? }

            /// Slots of the `Ints` column.
            #[derive(Debug, Clone, Copy)]
            pub enum Int { $($($int),*)? }

            /// Slots of the `Floats` column.
            #[derive(Debug, Clone, Copy)]
            pub enum Float { $($($float),*)? }

            /// Named ranges in the `Ranges` column.
            #[derive(Debug, Clone, Copy)]
            pub enum Range { $($($range),*)? }

            /// Bits of a flags slot.
            #[derive(Debug, Clone, Copy)]
            pub enum Flag { $($($flag),*)? }

            const OWN: &[(&str, Elem)] = &[$($((stringify!($col), Elem::$elem)),*)?];

            pub const COLUMNS: &[(&str, Elem)] = &{
                let mut all = [("", Elem::U8); COMMON.len() + OWN.len()];
                let mut i = 0;
                while i < COMMON.len() {
                    all[i] = COMMON[i];
                    i += 1;
                }
                while i < all.len() {
                    all[i] = OWN[i - COMMON.len()];
                    i += 1;
                }
                all
            };

            pub const SCHEMA: Kind = Kind {
                number: KIND,
                ints: &[$($(stringify!($int)),*)?],
                floats: &[$($(stringify!($float)),*)?],
                ranges: &[$($(stringify!($range)),*)?],
                flags: &[$($(stringify!($flag)),*)?],
                columns: COLUMNS,
            };

            impl From<Col> for usize {
                fn from(col: Col) -> usize { col as usize }
            }

            impl From<Int> for usize {
                fn from(slot: Int) -> usize { slot as usize }
            }

            impl From<Float> for usize {
                fn from(slot: Float) -> usize { slot as usize }
            }
        }
    };
}

// Layout and raw pages share their first columns, the text layer, at the same
// positions, so one reader serves both.
macro_rules! text_layer {
    ($module:ident, $kind:expr, ints: $ints:tt, floats: $floats:tt $(, columns: [$($rest:tt)*])?) => {
        schema!($module, $kind, ints: $ints, floats: $floats, columns: [
            BoxBbox: F64, BoxMode: I32, BoxText: I32, BoxLines: I32,
            LineBbox: F64, LineOrient: I32, LineText: I32, LineChars: I32,
            CharX0: F64, CharY0: F64, CharX1: F64, CharY1: F64,
            CharSize: F64, CharFont: I32, CharUpright: U8, CharText: I32,
            $($($rest)*)?
        ]);
    };
}

text_layer!(layout, 1,
    ints: [PageNumber, TextEnd],
    floats: [Rotate, X0, Y0, X1, Y1]);

text_layer!(raw, 2,
ints: [PageIndex, ObjectId, Rotation, LabelStart, LabelEnd, TextEnd],
floats: [UserUnit, X0, Y0, X1, Y1],
columns: [
    LineRawText: I32,
    CharAdvance: F64, CharMatrix: F64, CharMcid: I32,
    CharTag: I32, CharNcs: I32, CharScs: I32,
    CharNcolor: I32, CharScolor: I32, ColorPool: F64,
    PageBoxes: F64, PageBoxPresent: U8,
    TableBboxTopDown: F64, TableShape: I32, TableCells: I32,
    CellIndex: I32, CellBboxTopDown: F64, CellText: I32,
]);

schema!(page_text, 3,
ints: [PageNumber],
floats: [X0, Y0, X1, Y1],
columns: [
    BoxBbox: F64, BoxText: I32, BoxTextWithoutBidi: I32, BoxLines: I32,
    LineBbox: F64, LineText: I32, LineTextWithoutBidi: I32,
    GlyphFont: I32, GlyphText: I32, GlyphSize: F64,
]);

schema!(summary, 4,
    ints: [PageNumber],
    floats: [Rotate, X0, Y0, X1, Y1]);

schema!(tables, 5,
ints: [PageNumber],
columns: [
    TableBbox: F64, TableShape: I32, TableCells: I32,
    CellIndex: I32, CellBbox: F64, CellText: I32,
]);

schema!(table_rows, 6,
    ints: [PageNumber],
    columns: [CellText: I32, RowOffsets: I32, TableOffsets: I32]);

schema!(metadata, 7,
    ints: [FileSize, PageCount, Flags],
    ranges: [
        Title, Author, Subject, Keywords, Creator, Producer,
        CreationDateRaw, CreationDateIso, ModificationDateRaw, ModificationDateIso,
        VersionHeader, VersionCatalog, Form, Xmp,
    ],
    flags: [
        Encrypted, Printable, Modifiable, Extractable, Linearized,
        Tagged, UserProperties, Suspects, HasJavascript,
    ],
    columns: [Ranges: I32, InfoKeys: I32, InfoValues: I32]);

schema!(text, 8, columns: [Utf8: U8]);

/// Every kind.
pub const KINDS: &[&Kind] = &[
    &layout::SCHEMA,
    &raw::SCHEMA,
    &page_text::SCHEMA,
    &summary::SCHEMA,
    &tables::SCHEMA,
    &table_rows::SCHEMA,
    &metadata::SCHEMA,
    &text::SCHEMA,
];

/// The absent range.
pub const NONE: [i32; 2] = [-1, -1];

/// A count or offset as an `i32`, the width Java indexes arrays with.
pub(crate) fn to_i32(value: usize, what: &'static str) -> FfiResult<i32> {
    i32::try_from(value)
        .map_err(|_| FfiError::new(Code::RuntimeError, format!("{what} exceeds i32")))
}

/// Builds one batch.
///
/// `Text` holds the page's contiguous text (a page made of boxes made of
/// lines) first; strings that cannot be ranges into it are deferred and
/// appended after it on finish, and their ranges patched then.
pub struct Builder {
    kind: u32,
    columns: Vec<Column>,
    text: Vec<u16>,
    deferred: Vec<(usize, usize, String)>,
    strings: Vec<u16>,
    string_off: Vec<i32>,
    /// Each distinct string once, shared by the lookup and the id-ordered list.
    ids: FxHashMap<Rc<str>, i32>,
    keys: Vec<Rc<str>>,
    /// The id of each ASCII character, so single-byte glyphs skip hashing.
    ascii_ids: [i32; 128],
    last: [i32; 8],
}

impl Builder {
    pub fn new(kind: &Kind) -> Self {
        let mut columns: Vec<Column> = kind
            .columns
            .iter()
            .map(|(_, elem)| Column::new(*elem))
            .collect();
        columns[common::INTS] = Column::I64(vec![0; kind.ints.len()]);
        columns[common::FLOATS] = Column::F64(vec![0.0; kind.floats.len()]);
        Self {
            kind: kind.number,
            columns,
            text: Vec::new(),
            deferred: Vec::new(),
            strings: Vec::new(),
            string_off: vec![0],
            ids: FxHashMap::default(),
            keys: Vec::new(),
            ascii_ids: [-1; 128],
            last: [-1; 8],
        }
    }

    pub fn f64s(&mut self, col: impl Into<usize>) -> &mut Vec<f64> {
        match &mut self.columns[col.into()] {
            Column::F64(v) => v,
            other => panic!("column is not F64: {other:?}"),
        }
    }

    pub fn i32s(&mut self, col: impl Into<usize>) -> &mut Vec<i32> {
        match &mut self.columns[col.into()] {
            Column::I32(v) => v,
            other => panic!("column is not I32: {other:?}"),
        }
    }

    pub fn u8s(&mut self, col: impl Into<usize>) -> &mut Vec<u8> {
        match &mut self.columns[col.into()] {
            Column::U8(v) => v,
            other => panic!("column is not U8: {other:?}"),
        }
    }

    /// Set a named `Ints` slot.
    pub fn int(&mut self, slot: impl Into<usize>, value: i64) {
        match &mut self.columns[common::INTS] {
            Column::I64(v) => v[slot.into()] = value,
            _ => unreachable!("Ints is always I64"),
        }
    }

    /// Set a named `Floats` slot.
    pub fn float(&mut self, slot: impl Into<usize>, value: f64) {
        match &mut self.columns[common::FLOATS] {
            Column::F64(v) => v[slot.into()] = value,
            _ => unreachable!("Floats is always F64"),
        }
    }

    /// Set four consecutive `Floats` slots from a box.
    pub fn float_box(&mut self, first: impl Into<usize>, rect: (f64, f64, f64, f64)) {
        let first = first.into();
        for (offset, value) in <[f64; 4]>::from(rect).into_iter().enumerate() {
            self.float(first + offset, value);
        }
    }

    /// Reserve `n` more elements in each listed column.
    pub fn reserve(&mut self, cols: &[usize], n: usize) {
        for &col in cols {
            match &mut self.columns[col] {
                Column::F64(v) => v.reserve(n),
                Column::I32(v) => v.reserve(n),
                Column::I64(v) => v.reserve(n),
                Column::U8(v) => v.reserve(n),
                Column::U16(v) => v.reserve(n),
            }
        }
    }

    pub fn reserve_text(&mut self, units: usize) {
        self.text.reserve(units);
    }

    /// The current end of the text, as a range bound.
    pub fn text_pos(&self) -> FfiResult<i32> {
        to_i32(self.text.len(), "text")
    }

    /// Append to the text and return its range.
    pub fn text(&mut self, value: &str) -> FfiResult<[i32; 2]> {
        let start = self.text_pos()?;
        self.text.extend(value.encode_utf16());
        Ok([start, self.text_pos()?])
    }

    /// Append one UTF-16 unit to the text.
    pub fn text_unit(&mut self, unit: u16) {
        self.text.push(unit);
    }

    /// Append an optional value to the text.
    pub fn text_opt(&mut self, value: Option<&str>) -> FfiResult<[i32; 2]> {
        value.map_or(Ok(NONE), |value| self.text(value))
    }

    /// Push a range for `value` onto range column `col`, written after the
    /// page's contiguous text when the batch finishes.
    pub fn defer(&mut self, col: impl Into<usize>, value: String) {
        let col = col.into();
        let at = self.i32s(col).len();
        self.i32s(col).extend(NONE);
        self.deferred.push((col, at, value));
    }

    /// The id of `value` in the batch's string table. `slot` caches the last
    /// hit, so runs of the same font cost one comparison.
    pub fn intern(&mut self, slot: usize, value: &str) -> FfiResult<i32> {
        let last = self.last[slot];
        if last >= 0 && *self.keys[last as usize] == *value {
            return Ok(last);
        }
        let id = self.intern_new(value)?;
        self.last[slot] = id;
        Ok(id)
    }

    /// Like [`Self::intern`], with `-1` for absent.
    pub fn intern_opt(&mut self, slot: usize, value: Option<&str>) -> FfiResult<i32> {
        value.map_or(Ok(-1), |value| self.intern(slot, value))
    }

    /// Intern a glyph's text: a direct table for ASCII characters, the hash
    /// map for anything else.
    pub fn intern_glyph(&mut self, value: &str) -> FfiResult<i32> {
        if let [byte] = value.as_bytes()
            && byte.is_ascii()
        {
            let index = usize::from(*byte);
            if self.ascii_ids[index] < 0 {
                self.ascii_ids[index] = self.intern_new(value)?;
            }
            return Ok(self.ascii_ids[index]);
        }
        self.intern_new(value)
    }

    fn intern_new(&mut self, value: &str) -> FfiResult<i32> {
        if let Some(&id) = self.ids.get(value) {
            return Ok(id);
        }
        let id = to_i32(self.keys.len(), "string table")?;
        let key: Rc<str> = value.into();
        self.ids.insert(Rc::clone(&key), id);
        self.keys.push(key);
        self.strings.extend(value.encode_utf16());
        self.string_off
            .push(to_i32(self.strings.len(), "string table")?);
        Ok(id)
    }

    /// Finish the batch: append deferred strings after the text and patch
    /// their ranges.
    pub fn finish(mut self) -> FfiResult<Built> {
        for (col, at, value) in std::mem::take(&mut self.deferred) {
            let range = self.text(&value)?;
            self.i32s(col)[at..at + 2].copy_from_slice(&range);
        }
        self.columns[common::TEXT] = Column::U16(std::mem::take(&mut self.text));
        self.columns[common::STRINGS] = Column::U16(std::mem::take(&mut self.strings));
        self.columns[common::STRING_OFF] = Column::I32(std::mem::take(&mut self.string_off));
        Ok(Built {
            kind: self.kind,
            columns: self.columns,
        })
    }
}

/// Free a batch.
///
/// # Safety
/// `batch` must be null or come from this library, once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_batch_free(batch: *mut Batch) {
    unsafe { free_handle(batch) }
}

static SCHEMA_TEXT: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

fn schema_text(kind: &Kind) -> String {
    let mut lines = Vec::new();
    lines.extend(
        kind.columns
            .iter()
            .map(|(name, elem)| format!("col:{name}:{elem:?}")),
    );
    lines.extend(kind.ints.iter().map(|name| format!("int:{name}")));
    lines.extend(kind.floats.iter().map(|name| format!("float:{name}")));
    lines.extend(kind.ranges.iter().map(|name| format!("range:{name}")));
    lines.extend(kind.flags.iter().map(|name| format!("flag:{name}")));
    lines.join("\n")
}

/// A kind's columns, scalar slots, named ranges and flag bits, one per line:
/// `col:Name:ELEM`, `int:Name`, `float:Name`, `range:Name`, `flag:Name`, each
/// group in index order. The text is static; do not free it.
///
/// # Safety
/// The out-pointers must be writable; `err` must be null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_schema(
    kind: u32,
    out_ptr: *mut *const u8,
    out_len: *mut i64,
    err: *mut ErrorOut,
) -> i32 {
    unsafe {
        guard(err, || {
            let index = KINDS
                .iter()
                .position(|k| k.number == kind)
                .ok_or_else(|| FfiError::invalid("kind", format!("unknown batch kind {kind}")))?;
            let text = SCHEMA_TEXT.get_or_init(|| KINDS.iter().map(|k| schema_text(k)).collect());
            let text: &'static str = text[index].as_str();
            write_out(out_ptr, "out_ptr", text.as_ptr())?;
            write_out(out_len, "out_len", text.len() as i64)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deferred_strings_follow_the_text() {
        let mut b = Builder::new(&raw::SCHEMA);
        let line = b.text("ab").unwrap();
        b.i32s(raw::Col::LineText).extend(line);
        b.defer(raw::Col::LineRawText, "ba".to_owned());
        b.i32s(raw::Col::LineRawText).extend(NONE);
        let built = b.finish().unwrap();
        let Column::I32(raw_text) = &built.columns()[raw::Col::LineRawText as usize] else {
            panic!()
        };
        assert_eq!(raw_text, &[2, 4, -1, -1]);
        let Column::U16(text) = &built.columns()[common::TEXT] else {
            panic!()
        };
        assert_eq!(String::from_utf16(text).unwrap(), "abba");
    }

    #[test]
    fn interning_reuses_ids_and_records_offsets() {
        let mut b = Builder::new(&layout::SCHEMA);
        assert_eq!(b.intern(0, "F1").unwrap(), 0);
        assert_eq!(b.intern(0, "F1").unwrap(), 0);
        assert_eq!(b.intern(0, "F2").unwrap(), 1);
        assert_eq!(b.intern(1, "F1").unwrap(), 0);
        assert_eq!(b.intern_glyph("a").unwrap(), 2);
        assert_eq!(b.intern_glyph("a").unwrap(), 2);
        assert_eq!(b.intern_glyph("F1").unwrap(), 0, "glyphs share the table");
        assert_eq!(b.intern_glyph("é").unwrap(), 3);
        assert_eq!(b.intern_opt(2, None).unwrap(), -1);
        let built = b.finish().unwrap();
        let Column::I32(off) = &built.columns()[common::STRING_OFF] else {
            panic!()
        };
        assert_eq!(off, &[0, 2, 4, 5, 6]);
    }

    #[test]
    fn scalar_slots_are_sized_by_the_schema() {
        let mut b = Builder::new(&raw::SCHEMA);
        b.int(raw::Int::TextEnd as usize, 7);
        b.float_box(raw::Float::X0 as usize, (1.0, 2.0, 3.0, 4.0));
        let batch = b.finish().unwrap().into_batch();
        let Column::I64(ints) = &batch.columns()[common::INTS] else {
            panic!()
        };
        assert_eq!(ints.len(), raw::SCHEMA.ints.len());
        assert_eq!(ints[raw::Int::TextEnd as usize], 7);
        assert_eq!(batch.header.magic, MAGIC);
        assert_eq!(batch.header.ncols as usize, raw::COLUMNS.len());
    }

    #[test]
    fn layout_and_raw_share_the_text_layer() {
        let shared = layout::COLUMNS.len();
        assert_eq!(&raw::COLUMNS[..shared], layout::COLUMNS);
    }

    #[test]
    fn kinds_are_unique() {
        let mut kinds: Vec<u32> = KINDS.iter().map(|k| k.number).collect();
        kinds.sort_unstable();
        kinds.dedup();
        assert_eq!(kinds.len(), KINDS.len());
    }
}
