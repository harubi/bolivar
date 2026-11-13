//! Counting allocator for native-memory benchmarks (feature `count-alloc`).
//! The shared atomics slow multi-threaded work, so never time with it on.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static TOTAL: AtomicUsize = AtomicUsize::new(0);

struct Counting;

fn grow(n: usize) {
    TOTAL.fetch_add(n, Relaxed);
    let live = LIVE.fetch_add(n, Relaxed) + n;
    PEAK.fetch_max(live, Relaxed);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            grow(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        LIVE.fetch_sub(layout.size(), Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let new = unsafe { System.realloc(ptr, layout, size) };
        if !new.is_null() {
            LIVE.fetch_sub(layout.size(), Relaxed);
            grow(size);
        }
        new
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Start a measurement: peak becomes the live size, total becomes zero.
#[unsafe(no_mangle)]
pub extern "C" fn bolivar_alloc_reset() {
    PEAK.store(LIVE.load(Relaxed), Relaxed);
    TOTAL.store(0, Relaxed);
}

#[unsafe(no_mangle)]
pub extern "C" fn bolivar_alloc_live() -> i64 {
    LIVE.load(Relaxed) as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn bolivar_alloc_peak() -> i64 {
    PEAK.load(Relaxed) as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn bolivar_alloc_total() -> i64 {
    TOTAL.load(Relaxed) as i64
}
