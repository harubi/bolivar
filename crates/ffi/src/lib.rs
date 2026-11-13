//! C ABI for the bolivar PDF library.
//!
//! The JVM binding calls these functions through Java's Foreign Function &
//! Memory API. Results come back as columnar batches ([`batch`]): one per
//! page for cursors, one per call otherwise. See [`abi`] for the calling
//! convention every function follows.

pub mod abi;
#[cfg(feature = "count-alloc")]
pub mod alloc_count;
pub mod batch;
pub mod cursor;
pub mod document;
mod geometry;
mod metadata;
pub mod options;
mod pages;
mod text;

/// Bumped whenever a function, struct, column or code changes meaning.
pub const ABI_VERSION: u32 = 1;

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

/// The sizes of the structs the JVM lays out itself, in this order:
/// `Options`, `TableOptions`, `ErrorOut`, `BatchHeader`, `ColumnDesc`.
/// Writes at most `len` values; returns how many there are.
///
/// # Safety
/// `out` must be null or cover `len` writable values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_debug_struct_sizes(out: *mut i64, len: i64) -> i64 {
    let sizes = [
        size_of::<options::Options>(),
        size_of::<options::TableOptions>(),
        size_of::<abi::ErrorOut>(),
        size_of::<batch::BatchHeader>(),
        size_of::<batch::ColumnDesc>(),
    ];
    if !out.is_null() {
        for (index, size) in sizes.iter().enumerate().take(len.max(0) as usize) {
            unsafe { out.add(index).write(*size as i64) };
        }
    }
    sizes.len() as i64
}
