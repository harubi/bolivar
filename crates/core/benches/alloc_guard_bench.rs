use std::alloc::System;
use std::hint::black_box;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use stats_alloc::{Stats, StatsAlloc};
use stats_alloc_helper::{LockedAllocator, memory_measured};

use bolivar_core::document::PDFDocument;
use bolivar_core::high_level::{ExtractOptions, extract_pages_with_document};
use bolivar_core::layout::{LAParams, LTChar, LTLayoutContainer, TextBoxType};
use bolivar_core::table::{PageGeometry, TableSettings, extract_tables_from_ltpage};
use bolivar_core::utils::{HasBBox, Plane, Rect};

#[global_allocator]
static GLOBAL: LockedAllocator<System> = LockedAllocator::new(StatsAlloc::system());

static PRINTED: AtomicUsize = AtomicUsize::new(0);
const BIT_PLANE_ANY: usize = 1 << 0;
const BIT_GROUP_TEXTBOXES: usize = 1 << 1;
const BIT_TABLES_BASE: usize = 1 << 2;

const PAGE_BBOX: (f64, f64, f64, f64) = (0.0, 0.0, 612.0, 792.0);

fn report_once(bit: usize, label: &str, stats: Stats, iters: u64) {
    if PRINTED.fetch_or(bit, Ordering::Relaxed) & bit != 0 {
        return;
    }

    let iters = iters.max(1);
    let iters_f = iters as f64;
    let avg_allocs = stats.allocations as f64 / iters_f;
    let avg_deallocs = stats.deallocations as f64 / iters_f;
    let avg_reallocs = stats.reallocations as f64 / iters_f;
    let avg_bytes_alloc = stats.bytes_allocated as f64 / iters_f;
    let avg_bytes_dealloc = stats.bytes_deallocated as f64 / iters_f;
    let avg_bytes_realloc = stats.bytes_reallocated as f64 / iters_f;

    eprintln!(
        "alloc_guard {label}: iters={iters} total_allocs={} total_deallocs={} total_reallocs={} \
         bytes_allocated={} bytes_deallocated={} bytes_reallocated={} avg_allocs={:.3} \
         avg_deallocs={:.3} avg_reallocs={:.3} avg_bytes_allocated={:.3} \
         avg_bytes_deallocated={:.3} avg_bytes_reallocated={:.3}",
        stats.allocations,
        stats.deallocations,
        stats.reallocations,
        stats.bytes_allocated,
        stats.bytes_deallocated,
        stats.bytes_reallocated,
        avg_allocs,
        avg_deallocs,
        avg_reallocs,
        avg_bytes_alloc,
        avg_bytes_dealloc,
        avg_bytes_realloc,
    );
}

fn alloc_guard_iters() -> u64 {
    std::env::var("BOLIVAR_ALLOC_GUARD_ITERS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(10)
}

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

fn load_fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(&path)
        .unwrap_or_else(|e| panic!("Failed to load fixture {}: {}", path.display(), e))
}

fn generate_independent_boxes(n: usize) -> (LTLayoutContainer, LAParams, Vec<TextBoxType>) {
    let laparams = LAParams::default();
    let container = LTLayoutContainer::new(PAGE_BBOX);

    let columns = 3;
    let column_width = 180.0;
    let box_height = 40.0;
    let vertical_gap = 20.0;

    let mut all_chars = Vec::new();

    for i in 0..n {
        let col = i % columns;
        let row = i / columns;

        let left_margin = 36.0 + (col as f64 * (column_width + 20.0)) + (row as f64 * 3.0);
        let top = 720.0 - (row as f64 * (box_height + vertical_gap));

        let line_height = 10.0 + (i % 3) as f64 * 2.0;
        let char_width = 6.0 + (i % 4) as f64;

        let lines_in_box = 2 + (i % 2);
        for line_idx in 0..lines_in_box {
            let y0 = top - ((line_idx + 1) as f64 * (line_height + 2.0));
            let y1 = y0 + line_height;

            let chars_in_line = 5 + (i + line_idx) % 6;
            for char_idx in 0..chars_in_line {
                let x0 = left_margin + (char_idx as f64 * char_width);
                let x1 = x0 + char_width;

                all_chars.push(LTChar::new(
                    (x0, y0, x1, y1),
                    "a",
                    "Helvetica",
                    line_height,
                    true,
                    char_width,
                ));
            }
        }
    }

    let lines = container.group_objects(&laparams, &all_chars);
    let boxes = container.group_textlines(&laparams, lines);
    (container, laparams, boxes)
}

fn bench_alloc_plane_any(c: &mut Criterion) {
    let mut group = c.benchmark_group("alloc_guard_plane");

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

    group.bench_function("any_with_indices", |b| {
        b.iter(|| {
            let hit = plane.any_with_indices(query, |idx, _| idx == 0);
            black_box(hit);
        })
    });
    let iters = alloc_guard_iters();
    let stats = memory_measured(&GLOBAL, || {
        for _ in 0..iters {
            let hit = plane.any_with_indices(query, |idx, _| idx == 0);
            black_box(hit);
        }
    });
    report_once(BIT_PLANE_ANY, "plane_any_with_indices", stats, iters);

    group.finish();
}

fn bench_alloc_group_textboxes_exact(c: &mut Criterion) {
    let mut group = c.benchmark_group("alloc_guard_group_textboxes_exact");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(5));

    let (container, laparams, boxes) = generate_independent_boxes(120);

    group.bench_function("independent_120", |b| {
        b.iter(|| {
            let groups = container.group_textboxes_exact(&laparams, &boxes);
            black_box(groups.len());
        })
    });
    let iters = alloc_guard_iters();
    let stats = memory_measured(&GLOBAL, || {
        for _ in 0..iters {
            let groups = container.group_textboxes_exact(&laparams, &boxes);
            black_box(groups.len());
        }
    });
    report_once(
        BIT_GROUP_TEXTBOXES,
        "group_textboxes_exact_independent_120",
        stats,
        iters,
    );

    group.finish();
}

fn bench_alloc_extract_tables(c: &mut Criterion) {
    let mut group = c.benchmark_group("alloc_guard_extract_tables");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(5));

    let settings = TableSettings::default();
    let fixtures = [
        ("pdffill_demo", "pdfplumber/pdffill-demo.pdf"),
        (
            "nics_2015_11",
            "pdfplumber/nics-background-checks-2015-11.pdf",
        ),
    ];

    for (idx, (name, filename)) in fixtures.iter().enumerate() {
        let data = load_fixture(filename);
        let doc = PDFDocument::new(&data, "").expect("Failed to parse PDF");
        let opts = ExtractOptions {
            laparams: Some(LAParams::default()),
            ..Default::default()
        };
        let pages = extract_pages_with_document(&doc, opts).expect("Failed to extract pages");
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
        let label = format!("extract_tables_{name}");
        let bit = BIT_TABLES_BASE << idx;

        group.bench_with_input(BenchmarkId::from_parameter(name), &pages, |b, pages| {
            b.iter(|| {
                let mut table_count = 0usize;
                for (page, geom) in pages.iter().zip(geoms.iter()) {
                    let tables = extract_tables_from_ltpage(page, geom, &settings);
                    table_count += tables.len();
                }
                black_box(table_count);
            })
        });
        let iters = alloc_guard_iters();
        let stats = memory_measured(&GLOBAL, || {
            for _ in 0..iters {
                let mut table_count = 0usize;
                for (page, geom) in pages.iter().zip(geoms.iter()) {
                    let tables = extract_tables_from_ltpage(page, geom, &settings);
                    table_count += tables.len();
                }
                black_box(table_count);
            }
        });
        report_once(bit, label.as_str(), stats, iters);
    }

    group.finish();
}

criterion_group!(
    alloc_guard_benches,
    bench_alloc_plane_any,
    bench_alloc_group_textboxes_exact,
    bench_alloc_extract_tables
);
criterion_main!(alloc_guard_benches);
