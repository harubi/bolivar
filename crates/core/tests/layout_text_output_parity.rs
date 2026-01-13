use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};

use bolivar_core::layout::types::{
    LTAnno, LTChar, LTTextLine, LTTextLineHorizontal, TextLineElement,
};

static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static COUNT_ENABLED: Cell<bool> = const { Cell::new(false) };
}

struct CountingAlloc;

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNT_ENABLED.with(|enabled| enabled.get()) {
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

struct CountGuard {
    prev: bool,
}

impl Drop for CountGuard {
    fn drop(&mut self) {
        COUNT_ENABLED.with(|enabled| enabled.set(self.prev));
    }
}

fn reset_allocs() {
    ALLOC_COUNT.store(0, Ordering::Relaxed);
}

fn allocs() -> usize {
    ALLOC_COUNT.load(Ordering::Relaxed)
}

fn with_alloc_counter<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    let prev = COUNT_ENABLED.with(|enabled| {
        let prev = enabled.get();
        enabled.set(true);
        prev
    });
    let _guard = CountGuard { prev };
    f()
}

#[test]
fn textline_get_text_allocation_budget() {
    let mut line = LTTextLineHorizontal::new(0.1);
    let count = 200;
    for i in 0..count {
        let ch = LTChar::new(
            (i as f64, 0.0, i as f64 + 1.0, 10.0),
            "A",
            "F1",
            10.0,
            true,
            10.0,
        );
        line.add_element(TextLineElement::Char(Box::new(ch)));
    }
    line.analyze();

    reset_allocs();
    let text = with_alloc_counter(|| line.get_text());
    let used = allocs();

    assert_eq!(text.len(), count + 1);
    let max_allocs = 32;
    assert!(
        used <= max_allocs,
        "expected <= {max_allocs} allocations, got {used}"
    );
}

#[test]
fn textline_is_empty_allocation_free() {
    let mut line = LTTextLineHorizontal::new(0.1);
    line.set_bbox((0.0, 0.0, 1.0, 1.0));
    line.add_element(TextLineElement::Anno(LTAnno::new(" ")));

    reset_allocs();
    let _noise = black_box(Vec::<u8>::with_capacity(1));
    let empty = with_alloc_counter(|| line.is_empty());
    let used = allocs();

    assert!(empty);
    assert_eq!(used, 0, "expected 0 allocations, got {used}");
}
