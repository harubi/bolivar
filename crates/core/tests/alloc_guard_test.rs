use std::alloc::System;
use std::hint::black_box;

use bolivar_core::utils::{HasBBox, Plane, Rect};
use stats_alloc::{Stats, StatsAlloc};
use stats_alloc_helper::{LockedAllocator, memory_measured};

#[global_allocator]
static GLOBAL: LockedAllocator<System> = LockedAllocator::new(StatsAlloc::system());

#[derive(Clone, Copy)]
struct TestBox {
    bbox: Rect,
}

impl HasBBox for TestBox {
    fn x0(&self) -> f64 {
        self.bbox.0
    }

    fn y0(&self) -> f64 {
        self.bbox.1
    }

    fn x1(&self) -> f64 {
        self.bbox.2
    }

    fn y1(&self) -> f64 {
        self.bbox.3
    }
}

#[test]
fn test_plane_any_with_indices_no_allocs() {
    let mut plane = Plane::new((0.0, 0.0, 100.0, 100.0), 1);
    plane.extend(vec![
        TestBox {
            bbox: (0.0, 0.0, 10.0, 10.0),
        },
        TestBox {
            bbox: (20.0, 20.0, 30.0, 30.0),
        },
    ]);

    let query = (5.0, 5.0, 15.0, 15.0);
    let mut found = false;

    let stats = memory_measured(&GLOBAL, || {
        found = plane.any_with_indices(query, |idx, _| idx == 0);
        black_box(found);
    });

    assert!(found);
    assert_eq!(
        stats,
        Stats {
            allocations: 0,
            deallocations: 0,
            reallocations: 0,
            bytes_allocated: 0,
            bytes_deallocated: 0,
            bytes_reallocated: 0,
        }
    );
}
