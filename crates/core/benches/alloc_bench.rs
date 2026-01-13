#[path = "common/criterion.rs"]
mod bench_criterion;
#[path = "common/fixtures.rs"]
mod fixtures;
#[path = "common/group_heavy.rs"]
mod group_heavy;
#[path = "common/group_light.rs"]
mod group_light;
#[path = "common/tier.rs"]
mod bench_tier;

use std::alloc::System;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, Ordering};

use criterion::{BenchmarkId, criterion_group, criterion_main};
use stats_alloc::{Stats, StatsAlloc};
use stats_alloc_helper::{LockedAllocator, memory_measured};

use bolivar_core::document::PDFDocument;
use bolivar_core::high_level::{ExtractOptions, extract_text_with_document};
use bolivar_core::layout::{LAParams, LTChar, LTLayoutContainer};
use bolivar_core::table::{PageGeometry, TableSettings, extract_tables_from_ltpage};

use bench_criterion::{BenchCriterion, bench_criterion};
use bench_tier::bench_tier;
use fixtures::load_fixtures;
use group_heavy::configure_group_heavy;
use group_light::configure_group_light;

#[global_allocator]
static GLOBAL: LockedAllocator<System> = LockedAllocator::new(StatsAlloc::system());

static PRINT_ONCE: AtomicBool = AtomicBool::new(false);

fn report(label: &str, stats: Stats, iters: u64) {
    if PRINT_ONCE.swap(true, Ordering::Relaxed) {
        return;
    }
    let iters = iters.max(1) as f64;
    eprintln!(
        "alloc {label}: allocs={} deallocs={} bytes_allocated={} avg_allocs={:.2} avg_bytes={:.2}",
        stats.allocations,
        stats.deallocations,
        stats.bytes_allocated,
        stats.allocations as f64 / iters,
        stats.bytes_allocated as f64 / iters
    );
}

fn bench_alloc_extract_text(c: &mut BenchCriterion) {
    let tier = bench_tier();
    let fixtures = load_fixtures(Some("text"));

    let mut group = c.benchmark_group("alloc_extract_text");
    configure_group_heavy(&mut group, tier);

    for fx in fixtures {
        let doc = PDFDocument::new(&fx.bytes, "").expect("parse PDF");
        let options = ExtractOptions {
            laparams: Some(LAParams::default()),
            ..Default::default()
        };
        let iters = 10u64;

        group.bench_with_input(BenchmarkId::new("text", &fx.meta.id), &doc, |b, doc| {
            b.iter(|| {
                let text = extract_text_with_document(doc, options.clone()).expect("extract text");
                black_box(text.len());
            })
        });

        let stats = memory_measured(&GLOBAL, || {
            for _ in 0..iters {
                let text = extract_text_with_document(&doc, options.clone()).expect("extract text");
                black_box(text.len());
            }
        });
        report("extract_text", stats, iters);
    }

    group.finish();
}

fn bench_alloc_group_textboxes(c: &mut BenchCriterion) {
    let tier = bench_tier();
    let mut group = c.benchmark_group("alloc_group_textboxes_exact");
    configure_group_light(&mut group, tier);

    let laparams = LAParams::default();
    let container = LTLayoutContainer::new((0.0, 0.0, 612.0, 792.0));
    let mut chars = Vec::new();
    for i in 0..200 {
        let x0 = 36.0 + (i % 10) as f64 * 20.0;
        let y0 = 700.0 - (i / 10) as f64 * 12.0;
        let x1 = x0 + 8.0;
        let y1 = y0 + 10.0;
        chars.push(LTChar::new(
            (x0, y0, x1, y1),
            "a",
            "Helvetica",
            10.0,
            true,
            8.0,
        ));
    }
    let lines = container.group_objects(&laparams, &chars);
    let boxes = container.group_textlines(&laparams, lines);

    group.bench_function("group_textboxes_exact", |b| {
        b.iter(|| {
            let groups = container.group_textboxes_exact(&laparams, &boxes);
            black_box(groups.len());
        })
    });

    let iters = 20u64;
    let stats = memory_measured(&GLOBAL, || {
        for _ in 0..iters {
            let groups = container.group_textboxes_exact(&laparams, &boxes);
            black_box(groups.len());
        }
    });
    report("group_textboxes_exact", stats, iters);

    group.finish();
}

fn bench_alloc_extract_tables(c: &mut BenchCriterion) {
    let tier = bench_tier();
    let fixtures = load_fixtures(Some("tables"));
    let settings = TableSettings::default();

    let mut group = c.benchmark_group("alloc_extract_tables");
    configure_group_heavy(&mut group, tier);

    for fx in fixtures {
        let doc = PDFDocument::new(&fx.bytes, "").expect("parse PDF");
        let pages = bolivar_core::high_level::extract_pages_with_document(
            &doc,
            ExtractOptions {
                laparams: Some(LAParams::default()),
                ..Default::default()
            },
        )
        .expect("extract pages");

        let geoms: Vec<PageGeometry> = pages
            .iter()
            .map(|page| {
                let bbox = page.bbox();
                PageGeometry {
                    page_bbox: bbox,
                    mediabox: bbox,
                    initial_doctop: 0.0,
                    force_crop: false,
                }
            })
            .collect();

        group.bench_with_input(
            BenchmarkId::new("tables", &fx.meta.id),
            &pages,
            |b, pages| {
                b.iter(|| {
                    let mut count = 0usize;
                    for (page, geom) in pages.iter().zip(geoms.iter()) {
                        let tables = extract_tables_from_ltpage(page, geom, &settings);
                        count += tables.len();
                    }
                    black_box(count);
                })
            },
        );

        let iters = 10u64;
        let stats = memory_measured(&GLOBAL, || {
            for _ in 0..iters {
                let mut count = 0usize;
                for (page, geom) in pages.iter().zip(geoms.iter()) {
                    let tables = extract_tables_from_ltpage(page, geom, &settings);
                    count += tables.len();
                }
                black_box(count);
            }
        });
        report("extract_tables", stats, iters);
    }

    group.finish();
}

criterion_group!(
    name = alloc_benches;
    config = bench_criterion();
    targets = bench_alloc_extract_text, bench_alloc_group_textboxes, bench_alloc_extract_tables
);
criterion_main!(alloc_benches);
