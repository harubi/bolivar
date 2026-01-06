use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};

use bolivar_core::layout::params::LAParams;
use bolivar_core::layout::types::{LTChar, LTItem, LTLayoutContainer};

static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static COUNT_ENABLED: Cell<bool> = Cell::new(false);
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
fn analyze_limits_allocations_for_text_grouping() {
    let mut container = LTLayoutContainer::new((0.0, 0.0, 1000.0, 1000.0));
    let count = 200;
    for i in 0..count {
        let text = format!("C{i}");
        let ch = LTChar::new(
            (i as f64 * 2.0, 0.0, i as f64 * 2.0 + 1.0, 10.0),
            &text,
            "F1",
            10.0,
            true,
            10.0,
        );
        container.add(LTItem::Char(ch));
    }

    reset_allocs();
    let _noise = black_box(Vec::<u8>::with_capacity(1));
    with_alloc_counter(|| container.analyze(&LAParams::default()));
    let used = allocs();

    let max_allocs = count * 10;
    assert!(
        used <= max_allocs,
        "expected <= {max_allocs} allocations, got {used}"
    );
}
