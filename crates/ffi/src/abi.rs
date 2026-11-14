//! Calling convention shared by every exported function.
//!
//! Each function returns an `i32` status (`0` is success), writes results
//! through out-pointers, and reports failure detail through a caller-owned
//! [`ErrorOut`]. Detail is never kept in thread-local storage: a JVM virtual
//! thread may run on a different carrier thread for each native call.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicI64, Ordering};

use bolivar_core::{ErrorKind, InvalidSetting, PdfError};

/// Declares [`Code`] and [`Code::FAILURES`] from one list, so the names `bolivar_describe`
/// exports cannot drift from the enum.
macro_rules! codes {
    ($($variant:ident = $value:literal => $name:literal),* $(,)?) => {
        /// Status codes. Java mirrors these in `sa.ingenious.pdf.internal.Contract.Status`,
        /// which `bolivar_describe` checks at load.
        #[repr(i32)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Code {
            Ok = 0,
            $($variant = $value),*
        }

        impl Code {
            /// Every failure code, by the name Java gives it.
            pub const FAILURES: &[(&str, Code)] = &[$(($name, Code::$variant)),*];
        }
    };
}

codes! {
    InvalidPath = 1 => "INVALID_PATH",
    InvalidArgument = 2 => "INVALID_ARGUMENT",
    IoNotFound = 3 => "IO_NOT_FOUND",
    IoPermissionDenied = 4 => "IO_PERMISSION_DENIED",
    IoError = 5 => "IO_ERROR",
    SyntaxError = 6 => "SYNTAX_ERROR",
    EncryptionError = 7 => "ENCRYPTION_ERROR",
    MalformedPdf = 8 => "MALFORMED_PDF",
    DecodeError = 9 => "DECODE_ERROR",
    Cancelled = 10 => "CANCELLED",
    RuntimeError = 11 => "RUNTIME_ERROR",
    Panicked = 12 => "PANICKED",
    Unsupported = 13 => "UNSUPPORTED",
}

/// A failure: its code, a message, and the option field at fault, if any.
#[derive(Debug, Clone, PartialEq)]
pub struct FfiError {
    pub code: Code,
    pub message: String,
    pub field: Option<&'static str>,
}

impl FfiError {
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            field: None,
        }
    }

    /// An invalid option or argument, naming the field at fault.
    pub fn invalid(field: &'static str, message: impl Into<String>) -> Self {
        Self {
            code: Code::InvalidArgument,
            message: message.into(),
            field: Some(field),
        }
    }
}

impl From<PdfError> for FfiError {
    fn from(error: PdfError) -> Self {
        if let PdfError::InvalidSetting(setting) = error {
            return setting.into();
        }
        let code = match error.kind() {
            ErrorKind::NotFound => Code::IoNotFound,
            ErrorKind::PermissionDenied => Code::IoPermissionDenied,
            ErrorKind::Io => Code::IoError,
            ErrorKind::Syntax => Code::SyntaxError,
            ErrorKind::Decode => Code::DecodeError,
            ErrorKind::Encryption => Code::EncryptionError,
            ErrorKind::InvalidArgument => Code::InvalidArgument,
            ErrorKind::Unsupported => Code::Unsupported,
            ErrorKind::Cancelled => Code::Cancelled,
            ErrorKind::Runtime => Code::RuntimeError,
            ErrorKind::Malformed => Code::MalformedPdf,
        };
        Self::new(code, error.to_string())
    }
}

impl From<InvalidSetting> for FfiError {
    fn from(error: InvalidSetting) -> Self {
        Self::invalid(error.field, error.message)
    }
}

pub type FfiResult<T> = Result<T, FfiError>;

/// Failure detail written for the caller. Strings are UTF-16, owned by Rust,
/// and released by `bolivar_error_free`.
#[repr(C)]
#[derive(Debug)]
pub struct ErrorOut {
    pub code: i32,
    pub _pad: u32,
    pub field: *mut u16,
    pub field_len: i64,
    pub message: *mut u16,
    pub message_len: i64,
}

impl Default for ErrorOut {
    fn default() -> Self {
        Self {
            code: 0,
            _pad: 0,
            field: std::ptr::null_mut(),
            field_len: 0,
            message: std::ptr::null_mut(),
            message_len: 0,
        }
    }
}

fn leak_utf16(text: &str) -> (*mut u16, i64) {
    let units: Box<[u16]> = text.encode_utf16().collect();
    let len = units.len() as i64;
    (Box::into_raw(units).cast::<u16>(), len)
}

/// # Safety
/// `ptr` and `len` must come from [`leak_utf16`], once.
unsafe fn free_utf16(ptr: *mut u16, len: i64) {
    if !ptr.is_null() {
        let slice = std::ptr::slice_from_raw_parts_mut(ptr, len as usize);
        drop(unsafe { Box::from_raw(slice) });
    }
}

/// # Safety
/// `out` must be null or point to a writable `ErrorOut` whose strings are null
/// or were written by this module.
unsafe fn write_error(out: *mut ErrorOut, error: &FfiError) {
    let Some(out) = (unsafe { out.as_mut() }) else {
        return;
    };
    unsafe { free_error_strings(out) };
    out.code = error.code as i32;
    (out.field, out.field_len) = match error.field {
        Some(field) => leak_utf16(field),
        None => (std::ptr::null_mut(), 0),
    };
    (out.message, out.message_len) = leak_utf16(&error.message);
}

unsafe fn free_error_strings(out: &mut ErrorOut) {
    unsafe {
        free_utf16(out.field, out.field_len);
        free_utf16(out.message, out.message_len);
    }
    out.field = std::ptr::null_mut();
    out.field_len = 0;
    out.message = std::ptr::null_mut();
    out.message_len = 0;
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "native panic".to_owned()
    }
}

/// Run `body`, turning an error or a panic into a status code and detail.
///
/// # Safety
/// `err` must be null or point to a writable `ErrorOut`.
pub unsafe fn guard(err: *mut ErrorOut, body: impl FnOnce() -> FfiResult<()>) -> i32 {
    let error = match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(())) => return Code::Ok as i32,
        Ok(Err(error)) => error,
        Err(payload) => FfiError::new(Code::Panicked, panic_message(payload.as_ref())),
    };
    unsafe { write_error(err, &error) };
    error.code as i32
}

/// Release the strings of an `ErrorOut`. The struct itself stays with the caller.
///
/// # Safety
/// `err` must be null or point to an `ErrorOut` filled by this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_error_free(err: *mut ErrorOut) {
    no_unwind(|| {
        if let Some(out) = unsafe { err.as_mut() } {
            unsafe { free_error_strings(out) };
        }
    });
}

/// Run a function that returns nothing to C, swallowing a panic: unwinding
/// out of `extern "C"` would abort the JVM.
pub fn no_unwind(body: impl FnOnce()) {
    let _ = catch_unwind(AssertUnwindSafe(body));
}

/// Free a handle from [`into_handle`]. Null is ignored; a panic in `Drop`
/// is swallowed.
///
/// # Safety
/// `ptr` must be null or a live handle of type `T`, freed once, with no call
/// using it in progress.
pub unsafe fn free_handle<T>(ptr: *mut T) {
    no_unwind(|| drop(unsafe { from_handle(ptr) }));
}

// ---- pointer helpers ------------------------------------------------------

/// Write `value` through an out-pointer.
///
/// # Safety
/// `out` must be null or writable.
pub unsafe fn write_out<T>(out: *mut T, field: &'static str, value: T) -> FfiResult<()> {
    require_out(out, field)?;
    unsafe { out.write(value) };
    Ok(())
}

/// Refuse a null out-pointer, before any work whose result it would receive.
pub fn require_out<T>(out: *mut T, field: &'static str) -> FfiResult<()> {
    if out.is_null() {
        return Err(FfiError::invalid(field, "out pointer is null"));
    }
    Ok(())
}

/// Box `value` as a live handle and write it through `out`. Nothing is boxed
/// when `out` is null, so a bad call cannot leak a counted handle.
///
/// # Safety
/// `out` must be null or writable.
pub unsafe fn write_handle<T>(out: *mut *mut T, field: &'static str, value: T) -> FfiResult<()> {
    require_out(out, field)?;
    unsafe { out.write(into_handle(value)) };
    Ok(())
}

/// Borrow `len` elements at `ptr`. A zero length never dereferences.
///
/// # Safety
/// When `len > 0`, `ptr` must point to `len` readable elements.
pub unsafe fn slice<'a, T>(ptr: *const T, len: i64, field: &'static str) -> FfiResult<&'a [T]> {
    if len < 0 {
        return Err(FfiError::invalid(field, "length is negative"));
    }
    if len == 0 {
        return Ok(&[]);
    }
    if ptr.is_null() {
        return Err(FfiError::invalid(field, "pointer is null"));
    }
    Ok(unsafe { std::slice::from_raw_parts(ptr, len as usize) })
}

/// Read a C boolean byte. Anything but 0 or 1 is rejected rather than cast.
pub fn flag(byte: u8, field: &'static str) -> FfiResult<bool> {
    match byte {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(FfiError::invalid(
            field,
            format!("boolean byte must be 0 or 1, got {byte}"),
        )),
    }
}

/// Borrow a handle mutably.
///
/// # Safety
/// `ptr` must be null or a live handle of type `T` with no other borrow.
pub unsafe fn handle_mut<'a, T>(ptr: *mut T) -> FfiResult<&'a mut T> {
    unsafe { ptr.as_mut() }.ok_or_else(null_handle)
}

fn null_handle() -> FfiError {
    FfiError::invalid("handle", "handle is null")
}

/// Borrow a handle shared.
///
/// # Safety
/// `ptr` must be null or a live handle of type `T` with no mutable borrow.
pub unsafe fn handle_ref<'a, T>(ptr: *const T) -> FfiResult<&'a T> {
    unsafe { ptr.as_ref() }.ok_or_else(null_handle)
}

/// Take back a handle to consume it.
///
/// # Safety
/// As for [`from_handle`].
pub unsafe fn take_handle<T>(ptr: *mut T) -> FfiResult<Box<T>> {
    unsafe { from_handle(ptr) }.ok_or_else(null_handle)
}

// ---- live handle accounting ----------------------------------------------

static LIVE_HANDLES: AtomicI64 = AtomicI64::new(0);

/// Box a handle and count it as live.
pub fn into_handle<T>(value: T) -> *mut T {
    LIVE_HANDLES.fetch_add(1, Ordering::Relaxed);
    Box::into_raw(Box::new(value))
}

/// Take back a handle and count it as freed. Null is ignored.
///
/// # Safety
/// `ptr` must be null or come from [`into_handle`] with the same `T`, once.
pub unsafe fn from_handle<T>(ptr: *mut T) -> Option<Box<T>> {
    if ptr.is_null() {
        return None;
    }
    LIVE_HANDLES.fetch_sub(1, Ordering::Relaxed);
    Some(unsafe { Box::from_raw(ptr) })
}

/// The number of live handles of every kind, for leak tests.
#[unsafe(no_mangle)]
pub extern "C" fn bolivar_debug_live_handles() -> i64 {
    LIVE_HANDLES.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(out: &ErrorOut) -> String {
        let units = unsafe { std::slice::from_raw_parts(out.message, out.message_len as usize) };
        String::from_utf16(units).unwrap()
    }

    #[test]
    fn guard_reports_errors_with_field() {
        let mut out = ErrorOut::default();
        let code = unsafe {
            guard(&mut out, || {
                Err(FfiError::invalid("max_pages", "must be positive"))
            })
        };
        assert_eq!(code, Code::InvalidArgument as i32);
        assert_eq!(out.code, Code::InvalidArgument as i32);
        assert_eq!(message(&out), "must be positive");
        let field = unsafe { std::slice::from_raw_parts(out.field, out.field_len as usize) };
        assert_eq!(String::from_utf16(field).unwrap(), "max_pages");
        unsafe { bolivar_error_free(&mut out) };
        assert!(out.message.is_null() && out.field.is_null());
    }

    #[test]
    fn guard_turns_panics_into_a_status() {
        let mut out = ErrorOut::default();
        let code = unsafe { guard(&mut out, || panic!("boom")) };
        assert_eq!(code, Code::Panicked as i32);
        assert_eq!(message(&out), "boom");
        unsafe { bolivar_error_free(&mut out) };
    }

    #[test]
    fn guard_reuses_an_error_out_without_leaking() {
        let mut out = ErrorOut::default();
        for _ in 0..3 {
            unsafe { guard(&mut out, || Err(FfiError::new(Code::IoError, "again"))) };
        }
        assert_eq!(message(&out), "again");
        unsafe { bolivar_error_free(&mut out) };
    }

    #[test]
    fn flags_reject_bytes_other_than_zero_and_one() {
        assert_eq!(flag(0, "f"), Ok(false));
        assert_eq!(flag(1, "f"), Ok(true));
        assert_eq!(flag(2, "f").unwrap_err().field, Some("f"));
    }

    #[test]
    fn slices_reject_null_and_negative_lengths() {
        assert!(
            unsafe { slice::<u8>(std::ptr::null(), 0, "s") }
                .unwrap()
                .is_empty()
        );
        assert!(unsafe { slice::<u8>(std::ptr::null(), 1, "s") }.is_err());
        assert!(unsafe { slice::<u8>(std::ptr::null(), -1, "s") }.is_err());
    }

    #[test]
    fn pdf_errors_keep_their_message() {
        let error = FfiError::from(PdfError::SyntaxError("bad token".to_owned()));
        assert_eq!(error.code, Code::SyntaxError);
        assert!(error.message.contains("bad token"));
        assert_eq!(
            FfiError::from(PdfError::UnexpectedEof).code,
            Code::MalformedPdf
        );
        assert_eq!(FfiError::from(PdfError::Cancelled).code, Code::Cancelled);
    }
}
