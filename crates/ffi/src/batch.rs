//! Columnar batches handed to the JVM.
//!
//! A batch is a header and a directory of typed columns. Java reads the
//! header at the batch address, checks each column's element type and length,
//! copies the columns into Java arrays, and frees the batch. Every kind starts
//! with the same five columns ([`Common`]); the rest are declared per kind with
//! [`schema!`], and their names are exported so Java can check it agrees.
//!
//! Text is UTF-16 (Java's own string encoding, so no decoding on the JVM
//! side). Ranges into the text are `i32` start/end pairs; `-1, -1` is absent.

use rustc_hash::FxHashMap;

use crate::abi::{ErrorOut, FfiError, FfiResult, from_handle, guard, handle_ref, write_out};

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

impl Elem {
    const fn code(self) -> &'static str {
        match self {
            Self::F64 => "F64",
            Self::I32 => "I32",
            Self::I64 => "I64",
            Self::U8 => "U8",
            Self::U16 => "U16",
        }
    }
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

/// A finished batch. The header is the first field, so a batch pointer is a
/// header pointer.
#[repr(C)]
#[derive(Debug)]
pub struct Batch {
    header: BatchHeader,
    descs: Vec<ColumnDesc>,
    columns: Vec<Column>,
}

impl Batch {
    pub fn kind(&self) -> u32 {
        self.header.kind
    }

    pub fn columns(&self) -> &[Column] {
        &self.columns
    }
}

/// Declare a batch kind: its number and its columns after the common five.
macro_rules! schema {
    ($module:ident, $kind:expr, [$($col:ident : $elem:ident),* $(,)?]) => {
        pub mod $module {
            use crate::batch::Elem;
            pub const KIND: u32 = $kind;
            #[allow(dead_code)]
            #[derive(Debug, Clone, Copy)]
            #[repr(usize)]
            pub enum Col {
                Text, Strings, StringOff, Ints, Floats,
                $($col),*
            }
            pub const COLUMNS: &[(&str, Elem)] = &[
                ("Text", Elem::U16), ("Strings", Elem::U16), ("StringOff", Elem::I32),
                ("Ints", Elem::I64), ("Floats", Elem::F64),
                $((stringify!($col), Elem::$elem)),*
            ];
            impl From<Col> for usize {
                fn from(col: Col) -> usize { col as usize }
            }
        }
    };
}

/// Indices of the five columns every kind starts with.
pub mod common {
    pub const TEXT: usize = 0;
    pub const STRINGS: usize = 1;
    pub const STRING_OFF: usize = 2;
    pub const INTS: usize = 3;
    pub const FLOATS: usize = 4;
}

schema!(layout, 1, [
    BoxBbox: F64, BoxMode: I32, BoxText: I32, BoxLines: I32,
    LineBbox: F64, LineOrient: I32, LineText: I32, LineChars: I32,
    CharX0: F64, CharY0: F64, CharX1: F64, CharY1: F64,
    CharSize: F64, CharFont: I32, CharUpright: U8, CharText: I32,
]);

schema!(raw, 2, [
    PageBoxes: F64, PageBoxPresent: U8,
    BoxBbox: F64, BoxMode: I32, BoxText: I32, BoxLines: I32,
    LineBbox: F64, LineOrient: I32, LineRawText: I32, LineText: I32, LineChars: I32,
    CharX0: F64, CharY0: F64, CharX1: F64, CharY1: F64,
    CharSize: F64, CharFont: I32, CharUpright: U8, CharText: I32,
    CharAdvance: F64, CharMatrix: F64, CharMcid: I32,
    CharTag: I32, CharNcs: I32, CharScs: I32,
    CharNcolor: I32, CharScolor: I32, ColorPool: F64,
    TableBbox: F64, TableShape: I32, TableCells: I32,
    CellIndex: I32, CellBbox: F64, CellText: I32,
]);

schema!(page_text, 3, [
    BoxBbox: F64, BoxText: I32, BoxTextWithoutBidi: I32, BoxLines: I32,
    LineBbox: F64, LineText: I32, LineTextWithoutBidi: I32,
    Fonts: I32, DigitFont: I32, DigitSize: I32,
]);

schema!(summary, 4, []);

schema!(tables, 5, [
    TableBbox: F64, TableShape: I32, TableCells: I32,
    CellIndex: I32, CellBbox: F64, CellText: I32,
]);

schema!(table_rows, 6, [CellText: I32, RowOffsets: I32, TableOffsets: I32]);

schema!(metadata, 7, [Fields: I32, InfoKeys: I32, InfoValues: I32]);

schema!(text, 8, []);

/// Every kind's number and columns.
pub const KINDS: &[(u32, &[(&str, Elem)])] = &[
    (layout::KIND, layout::COLUMNS),
    (raw::KIND, raw::COLUMNS),
    (page_text::KIND, page_text::COLUMNS),
    (summary::KIND, summary::COLUMNS),
    (tables::KIND, tables::COLUMNS),
    (table_rows::KIND, table_rows::COLUMNS),
    (metadata::KIND, metadata::COLUMNS),
    (text::KIND, text::COLUMNS),
];

/// The absent range.
pub const NONE: [i32; 2] = [-1, -1];

/// A count or offset as an `i32`, the width Java indexes arrays with.
pub(crate) fn to_i32(value: usize, what: &'static str) -> FfiResult<i32> {
    i32::try_from(value).map_err(|_| {
        FfiError::new(
            crate::abi::Code::RuntimeError,
            format!("{what} exceeds i32"),
        )
    })
}

/// Builds one batch.
///
/// Text has two buffers. `text` holds runs that must stay contiguous (a
/// page's text, made of its boxes, made of its lines); `aux` holds the rest
/// (character text, raw line text, text without bidi). On finish, `aux` is
/// appended after `text` and every range in the `aux_columns` is shifted.
pub struct Builder {
    kind: u32,
    columns: Vec<Column>,
    text: Vec<u16>,
    aux: Vec<u16>,
    strings: Vec<u16>,
    string_off: Vec<i32>,
    ids: FxHashMap<Box<str>, i32>,
    keys: Vec<Box<str>>,
    last: [i32; 6],
}

impl Builder {
    pub fn new(kind: u32, schema: &[(&str, Elem)]) -> Self {
        Self {
            kind,
            columns: schema.iter().map(|(_, elem)| Column::new(*elem)).collect(),
            text: Vec::new(),
            aux: Vec::new(),
            strings: Vec::new(),
            string_off: vec![0],
            ids: FxHashMap::default(),
            keys: Vec::new(),
            last: [-1; 6],
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

    pub fn i64s(&mut self, col: impl Into<usize>) -> &mut Vec<i64> {
        match &mut self.columns[col.into()] {
            Column::I64(v) => v,
            other => panic!("column is not I64: {other:?}"),
        }
    }

    pub fn u8s(&mut self, col: impl Into<usize>) -> &mut Vec<u8> {
        match &mut self.columns[col.into()] {
            Column::U8(v) => v,
            other => panic!("column is not U8: {other:?}"),
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

    pub fn reserve_text(&mut self, units: usize, aux_units: usize) {
        self.text.reserve(units);
        self.aux.reserve(aux_units);
    }

    /// The current end of the contiguous text, as a range bound.
    pub fn text_pos(&self) -> FfiResult<i32> {
        to_i32(self.text.len(), "text")
    }

    /// Append to the contiguous text and return its range.
    pub fn text(&mut self, value: &str) -> FfiResult<[i32; 2]> {
        let start = self.text_pos()?;
        self.text.extend(value.encode_utf16());
        Ok([start, self.text_pos()?])
    }

    /// Append one UTF-16 unit to the contiguous text.
    pub fn text_unit(&mut self, unit: u16) {
        self.text.push(unit);
    }

    /// The current end of the aux text, as a range bound before shifting.
    pub fn aux_pos(&self) -> FfiResult<i32> {
        to_i32(self.aux.len(), "aux text")
    }

    /// Append to the aux text and return its (unshifted) range.
    pub fn aux(&mut self, value: &str) -> FfiResult<[i32; 2]> {
        let start = self.aux_pos()?;
        self.aux.extend(value.encode_utf16());
        Ok([start, self.aux_pos()?])
    }

    /// Append an optional value to the contiguous text.
    pub fn text_opt(&mut self, value: Option<&str>) -> FfiResult<[i32; 2]> {
        value.map_or(Ok(NONE), |value| self.text(value))
    }

    /// The id of `value` in the batch's string table. `slot` caches the last
    /// hit, so runs of the same font cost one comparison.
    pub fn intern(&mut self, slot: usize, value: &str) -> FfiResult<i32> {
        let last = self.last[slot];
        if last >= 0 && *self.keys[last as usize] == *value {
            return Ok(last);
        }
        let id = match self.ids.get(value) {
            Some(&id) => id,
            None => {
                let id = to_i32(self.keys.len(), "string table")?;
                self.ids.insert(value.into(), id);
                self.keys.push(value.into());
                self.strings.extend(value.encode_utf16());
                self.string_off
                    .push(to_i32(self.strings.len(), "string table")?);
                id
            }
        };
        self.last[slot] = id;
        Ok(id)
    }

    /// Like [`Self::intern`], with `-1` for absent.
    pub fn intern_opt(&mut self, slot: usize, value: Option<&str>) -> FfiResult<i32> {
        value.map_or(Ok(-1), |value| self.intern(slot, value))
    }

    /// Finish the batch, shifting the ranges in `aux_columns` past the
    /// contiguous text. Absent ranges (`-1`) stay absent.
    pub fn finish(mut self, aux_columns: &[usize]) -> FfiResult<Batch> {
        let shift = to_i32(self.text.len(), "text")?;
        to_i32(self.text.len() + self.aux.len(), "text")?;
        for &col in aux_columns {
            for value in self.i32s(col).iter_mut() {
                if *value >= 0 {
                    *value += shift;
                }
            }
        }
        let mut text = std::mem::take(&mut self.text);
        text.extend_from_slice(&self.aux);
        self.columns[common::TEXT] = Column::U16(text);
        self.columns[common::STRINGS] = Column::U16(std::mem::take(&mut self.strings));
        self.columns[common::STRING_OFF] = Column::I32(std::mem::take(&mut self.string_off));
        let descs: Vec<ColumnDesc> = self.columns.iter().map(Column::desc).collect();
        let header = BatchHeader {
            magic: MAGIC,
            abi_version: crate::ABI_VERSION,
            kind: self.kind,
            ncols: descs.len() as u32,
            cols: descs.as_ptr(),
        };
        Ok(Batch {
            header,
            descs,
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
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        drop(unsafe { from_handle(batch) });
    }));
}

/// The column names and element types of a kind, as `Name:ELEM` lines.
/// The text is static; do not free it.
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
                .position(|(k, _)| *k == kind)
                .ok_or_else(|| FfiError::invalid("kind", format!("unknown batch kind {kind}")))?;
            let text: &'static str = SCHEMA_TEXT.get_or_init(build_schema_text)[index].as_str();
            write_out(out_ptr, "out_ptr", text.as_ptr())?;
            write_out(out_len, "out_len", text.len() as i64)
        })
    }
}

static SCHEMA_TEXT: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

fn build_schema_text() -> Vec<String> {
    KINDS
        .iter()
        .map(|(_, columns)| {
            columns
                .iter()
                .map(|(name, elem)| format!("{name}:{}", elem.code()))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect()
}

/// Check a batch handle (used by tests through the C ABI).
///
/// # Safety
/// `batch` must be null or a live batch.
pub unsafe fn batch_ref<'a>(batch: *const Batch) -> FfiResult<&'a Batch> {
    unsafe { handle_ref(batch) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aux_ranges_shift_past_the_contiguous_text() {
        let mut b = Builder::new(layout::KIND, layout::COLUMNS);
        let line = b.text("ab").unwrap();
        let ch = b.aux("a").unwrap();
        b.i32s(layout::Col::LineText).extend(line);
        b.i32s(layout::Col::CharText).extend(ch);
        b.i32s(layout::Col::CharText).extend(NONE);
        let batch = b.finish(&[layout::Col::CharText.into()]).unwrap();
        let Column::I32(chars) = &batch.columns()[layout::Col::CharText as usize] else {
            panic!()
        };
        assert_eq!(chars, &[2, 3, -1, -1]);
        let Column::U16(text) = &batch.columns()[common::TEXT] else {
            panic!()
        };
        assert_eq!(String::from_utf16(text).unwrap(), "aba");
    }

    #[test]
    fn interning_reuses_ids_and_records_offsets() {
        let mut b = Builder::new(layout::KIND, layout::COLUMNS);
        assert_eq!(b.intern(0, "F1").unwrap(), 0);
        assert_eq!(b.intern(0, "F1").unwrap(), 0);
        assert_eq!(b.intern(0, "F2").unwrap(), 1);
        assert_eq!(b.intern(1, "F1").unwrap(), 0);
        assert_eq!(b.intern_opt(2, None).unwrap(), -1);
        let batch = b.finish(&[]).unwrap();
        let Column::I32(off) = &batch.columns()[common::STRING_OFF] else {
            panic!()
        };
        assert_eq!(off, &[0, 2, 4]);
    }

    #[test]
    fn headers_describe_every_column() {
        let batch = Builder::new(raw::KIND, raw::COLUMNS).finish(&[]).unwrap();
        assert_eq!(batch.header.magic, MAGIC);
        assert_eq!(batch.header.ncols as usize, raw::COLUMNS.len());
        assert_eq!(batch.descs.len(), raw::COLUMNS.len());
        for (index, (desc, (_, elem))) in batch.descs.iter().zip(raw::COLUMNS).enumerate() {
            assert_eq!(desc.elem, *elem as u32);
            // The string offsets always start with 0.
            let expected = i64::from(index == common::STRING_OFF);
            assert_eq!(desc.len, expected);
        }
    }

    #[test]
    fn kinds_are_unique() {
        let mut kinds: Vec<u32> = KINDS.iter().map(|(k, _)| *k).collect();
        kinds.sort_unstable();
        kinds.dedup();
        assert_eq!(kinds.len(), KINDS.len());
    }
}
