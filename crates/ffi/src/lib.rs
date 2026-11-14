//! C ABI for the bolivar PDF library.
//!
//! The JVM binding calls these functions through Java's Foreign Function &
//! Memory API. Results come back as columnar batches ([`batch`]): one per
//! page for cursors, one per call otherwise. See [`abi`] for the calling
//! convention every function follows.
//!
//! Everything the JVM mirrors by hand (batch schemas, struct layouts, option
//! bits, strategy and status codes) is exported by name through
//! `bolivar_schema` and `bolivar_describe`, and checked by the JVM tests.

pub mod abi;
#[cfg(feature = "count-alloc")]
pub mod alloc_count;
pub mod batch;
pub mod cursor;
pub mod document;
mod metadata;
pub mod options;
mod pages;

use std::sync::OnceLock;

use abi::{Code, ErrorOut, FfiError, guard, write_out};
use batch::{BatchHeader, ColumnDesc};
use options::{CursorOptions, Options, TableOptions};

/// Bumped whenever a function, struct, column or code changes meaning.
pub const ABI_VERSION: u32 = 3;

/// The ABI version this library speaks.
#[unsafe(no_mangle)]
pub extern "C" fn bolivar_abi_version() -> u32 {
    ABI_VERSION
}

static VERSION: &str = env!("CARGO_PKG_VERSION");

/// The library version as UTF-8. The text is static; do not free it.
///
/// # Safety
/// The out-pointers must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_version(out_ptr: *mut *const u8, out_len: *mut i64) {
    if !out_ptr.is_null() && !out_len.is_null() {
        unsafe {
            out_ptr.write(VERSION.as_ptr());
            out_len.write(VERSION.len() as i64);
        }
    }
}

const fn field_size<T, F>(_: fn(&T) -> &F) -> usize {
    size_of::<F>()
}

/// `size:N` then one `field:name:offset:size` line per field.
macro_rules! layout {
    ($ty:ty { $($field:ident),* $(,)? }) => {{
        let mut lines = vec![format!("size:{}", size_of::<$ty>())];
        $(lines.push(format!(
            "field:{}:{}:{}",
            stringify!($field),
            std::mem::offset_of!($ty, $field),
            field_size(|value: &$ty| &value.$field),
        ));)*
        lines.join("\n")
    }};
}

fn named<T: std::fmt::Display>(
    entries: impl IntoIterator<Item = (impl std::fmt::Display, T)>,
) -> String {
    entries
        .into_iter()
        .map(|(name, value)| format!("value:{name}:{value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// What `bolivar_describe` can describe, by name.
/// Builds one description.
type Describe = fn() -> String;

const DESCRIPTIONS: &[(&str, Describe)] = &[
    ("Options", || {
        layout!(Options {
            struct_size,
            present,
            caching,
            bidi,
            detect_vertical,
            all_texts,
            max_pages,
            line_overlap,
            char_margin,
            line_margin,
            word_margin,
            boxes_flow,
            password,
            password_len,
            pages,
            pages_len,
        })
    }),
    ("TableOptions", || {
        layout!(TableOptions {
            struct_size,
            present,
            vertical_strategy,
            horizontal_strategy,
            snap,
            snap_x,
            snap_y,
            join,
            join_x,
            join_y,
            intersection,
            intersection_x,
            intersection_y,
            crop,
            first_page_crop,
            explicit_vertical,
            explicit_vertical_len,
            explicit_horizontal,
            explicit_horizontal_len,
        })
    }),
    ("CursorOptions", || {
        layout!(CursorOptions {
            struct_size,
            present,
            skip,
            max_pages,
            read_ahead,
            pages,
            pages_len,
        })
    }),
    ("ErrorOut", || {
        layout!(ErrorOut {
            code,
            field,
            field_len,
            message,
            message_len
        })
    }),
    ("BatchHeader", || {
        layout!(BatchHeader {
            magic,
            abi_version,
            kind,
            ncols,
            cols
        })
    }),
    ("ColumnDesc", || layout!(ColumnDesc { ptr, len, elem })),
    ("OptionBits", || named(options::opt::NAMES.iter().copied())),
    ("TableOptionBits", || {
        named(options::topt::NAMES.iter().copied())
    }),
    ("CursorOptionBits", || {
        named(options::copt::NAMES.iter().copied())
    }),
    ("TableStrategy", || {
        named(
            options::STRATEGIES
                .iter()
                .enumerate()
                .map(|(code, (name, _))| (name, code)),
        )
    }),
    ("Status", || {
        named(
            Code::FAILURES
                .iter()
                .map(|&(name, code)| (name, code as i32)),
        )
    }),
];

static DESCRIBED: OnceLock<Vec<String>> = OnceLock::new();

/// A description of something the JVM mirrors by hand, as static UTF-8
/// lines: struct layouts (`size:N`, `field:name:offset:size`) and named
/// values (`value:NAME:N`). `name` is one of `Options`, `TableOptions`,
/// `CursorOptions`, `ErrorOut`, `BatchHeader`, `ColumnDesc`, `OptionBits`,
/// `TableOptionBits`, `CursorOptionBits`, `TableStrategy`, `Status`.
///
/// # Safety
/// `name` must cover `name_len` bytes; the out-pointers must be writable;
/// `err` must be null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_describe(
    name: *const u8,
    name_len: i64,
    out_ptr: *mut *const u8,
    out_len: *mut i64,
    err: *mut ErrorOut,
) -> i32 {
    unsafe {
        guard(err, || {
            let name = std::str::from_utf8(abi::slice(name, name_len, "name")?)
                .map_err(|_| FfiError::invalid("name", "name must be UTF-8"))?;
            let index = DESCRIPTIONS
                .iter()
                .position(|(known, _)| *known == name)
                .ok_or_else(|| FfiError::invalid("name", format!("nothing named {name}")))?;
            let all = DESCRIBED.get_or_init(|| {
                DESCRIPTIONS
                    .iter()
                    .map(|(_, describe)| describe())
                    .collect()
            });
            let text: &'static str = all[index].as_str();
            write_out(out_ptr, "out_ptr", text.as_ptr())?;
            write_out(out_len, "out_len", text.len() as i64)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_description_exists() {
        for (name, describe) in DESCRIPTIONS {
            assert!(!describe().is_empty(), "{name}");
        }
        assert!((DESCRIPTIONS[0].1)().starts_with("size:96\nfield:struct_size:0:4"));
    }
}
