mod common;

use std::hint::black_box;

use criterion::{BenchmarkId, criterion_group, criterion_main};

use bolivar_core::utils::{HasBBox, Plane, Rect};

use common::{
    BenchCriterion, GroupWeight, XorShift64, bench_config, bench_criterion, configure_group,
    pages_throughput,
};

#[derive(Clone, Copy)]
struct BoxItem {
    bbox: Rect,
}

impl HasBBox for BoxItem {
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

fn gen_boxes(seed: u64, n: usize) -> Vec<BoxItem> {
    let mut rng = XorShift64::new(seed);
    let mut items = Vec::with_capacity(n);
    for _ in 0..n {
        let x0 = rng.gen_f64(0.0, 1000.0);
        let y0 = rng.gen_f64(0.0, 1000.0);
        let w = rng.gen_f64(5.0, 50.0);
        let h = rng.gen_f64(5.0, 50.0);
        items.push(BoxItem {
            bbox: (x0, y0, x0 + w, y0 + h),
        });
    }
    items
}

fn bench_plane_queries(c: &mut BenchCriterion) {
    let cfg = bench_config();
    let sizes: &[usize] = if cfg.tier == common::BenchTier::Quick {
        &[1_000, 5_000]
    } else {
        &[1_000, 5_000, 20_000]
    };

    let mut group = c.benchmark_group("micro_plane_queries");
    configure_group(&mut group, &cfg, GroupWeight::Light);

    for &n in sizes {
        let boxes = gen_boxes(cfg.seed ^ (n as u64), n);
        let mut plane = Plane::new((0.0, 0.0, 1000.0, 1000.0), 1);
        plane.extend(boxes);
        let query = (200.0, 200.0, 400.0, 400.0);

        group.throughput(pages_throughput(n));
        group.bench_with_input(BenchmarkId::new("find_with_indices", n), &query, |b, q| {
            b.iter(|| {
                let hits = plane.find_with_indices(*q);
                black_box(hits.len());
            })
        });

        group.bench_with_input(BenchmarkId::new("any_with_indices", n), &query, |b, q| {
            b.iter(|| {
                let hit = plane.any_with_indices(*q, |idx, _| idx % 7 == 0);
                black_box(hit);
            })
        });

        group.bench_with_input(BenchmarkId::new("neighbors", n), &query, |b, q| {
            b.iter(|| {
                let hits = plane.neighbors(*q, 8);
                black_box(hits.len());
            })
        });
    }

    group.finish();
}

fn bench_plane_add_remove(c: &mut BenchCriterion) {
    let cfg = bench_config();
    let sizes: &[usize] = if cfg.tier == common::BenchTier::Quick {
        &[1_000]
    } else {
        &[1_000, 5_000]
    };

    let mut group = c.benchmark_group("micro_plane_add_remove");
    configure_group(&mut group, &cfg, GroupWeight::Light);

    for &n in sizes {
        let items = gen_boxes(cfg.seed ^ 0xBAD5EED, n);
        group.bench_with_input(BenchmarkId::new("extend", n), &items, |b, items| {
            b.iter(|| {
                let mut plane = Plane::new((0.0, 0.0, 1000.0, 1000.0), 1);
                plane.extend(items.clone());
                black_box(plane.len());
            })
        });

        group.bench_with_input(BenchmarkId::new("add_remove", n), &items, |b, items| {
            b.iter(|| {
                let mut plane = Plane::new((0.0, 0.0, 1000.0, 1000.0), 1);
                for item in items.iter().cloned() {
                    plane.add(item);
                }
                for id in (0..n).step_by(5) {
                    let _ = plane.remove_by_id(id);
                }
                black_box(plane.len());
            })
        });
    }

    group.finish();
}

criterion_group!(
    name = micro_plane_benches;
    config = bench_criterion();
    targets = bench_plane_queries, bench_plane_add_remove
);
criterion_main!(micro_plane_benches);
