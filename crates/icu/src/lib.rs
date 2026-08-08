use std::ffi::c_void;
use std::ptr::NonNull;

unsafe extern "C" {
    fn bolivar_icu_bidi_open() -> *mut c_void;
    fn bolivar_icu_bidi_close(bidi: *mut c_void);
    fn bolivar_icu_bidi_inverse(
        bidi: *mut c_void,
        source: *const u16,
        source_length: i32,
        paragraph_level: u8,
        destination: *mut u16,
        destination_capacity: i32,
        output_to_source: *mut i32,
    ) -> i32;
}

pub struct Bidi {
    context: NonNull<c_void>,
}

impl Bidi {
    pub fn new() -> Option<Self> {
        NonNull::new(unsafe { bolivar_icu_bidi_open() }).map(|context| Self { context })
    }

    pub fn inverse(
        &mut self,
        source: &[u16],
        paragraph_level: u8,
        destination: &mut [u16],
        output_to_source: Option<&mut [i32]>,
    ) -> Option<usize> {
        let source_length = i32::try_from(source.len()).ok()?;
        let destination_capacity = i32::try_from(destination.len()).ok()?;
        let output_to_source = match output_to_source {
            Some(mapping) if mapping.len() >= source.len() => mapping.as_mut_ptr(),
            Some(_) => return None,
            None => std::ptr::null_mut(),
        };
        let output_length = unsafe {
            bolivar_icu_bidi_inverse(
                self.context.as_ptr(),
                source.as_ptr(),
                source_length,
                paragraph_level,
                destination.as_mut_ptr(),
                destination_capacity,
                output_to_source,
            )
        };
        usize::try_from(output_length).ok()
    }
}

impl Drop for Bidi {
    fn drop(&mut self) {
        unsafe { bolivar_icu_bidi_close(self.context.as_ptr()) };
    }
}
