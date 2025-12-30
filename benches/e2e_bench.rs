//! End-to-end benchmarks for text extraction.
//!
//! These benchmarks measure the full text extraction pipeline using the public API
//! from `src/high_level.rs`:
//! - `extract_text()` - Extract all text from a PDF as a String
//! - `extract_pages()` - Iterator over analyzed LTPage objects

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;

use bolivar::high_level::{ExtractOptions, extract_pages, extract_text};
use bolivar::layout::LAParams;

/// Load a test fixture by name.
fn load_fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("Failed to load fixture {}: {}", path, e))
}

// =============================================================================
// Benchmark Groups
// =============================================================================

/// Benchmark extract_text - full text extraction pipeline.
///
/// Tests various PDF files:
/// - simple1-5.pdf: Various simple PDFs
/// - jo.pdf: Japanese text with CID fonts
fn bench_e2e_extract_text(c: &mut Criterion) {
    let mut group = c.benchmark_group("e2e_extract_text");

    let fixtures = [
        ("simple1", "simple1.pdf"),
        ("simple2", "simple2.pdf"),
        ("simple3", "simple3.pdf"),
        ("simple4", "simple4.pdf"),
        ("simple5", "simple5.pdf"),
        ("jo", "jo.pdf"),
    ];

    for (name, filename) in fixtures {
        let data = load_fixture(filename);

        group.bench_with_input(BenchmarkId::from_parameter(name), &data, |b, data| {
            b.iter(|| extract_text(black_box(data), None).expect("Failed to extract text"))
        });
    }

    group.finish();
}

/// Benchmark LAParams variations - compare different layout analysis configurations.
///
/// Tests:
/// - default: Standard LAParams with boxes_flow=0.5
/// - no_boxes_flow: boxes_flow=None (faster, simpler layout)
/// - detect_vertical: detect_vertical=true (for vertical text)
fn bench_e2e_laparams(c: &mut Criterion) {
    let mut group = c.benchmark_group("e2e_laparams");

    // Use simple4.pdf as a representative test file
    let data = load_fixture("simple4.pdf");

    // Default LAParams
    let default_opts = ExtractOptions {
        laparams: Some(LAParams::default()),
        ..Default::default()
    };

    group.bench_with_input(BenchmarkId::from_parameter("default"), &data, |b, data| {
        b.iter(|| {
            extract_text(black_box(data), Some(default_opts.clone()))
                .expect("Failed to extract text")
        })
    });

    // No boxes_flow (disables advanced layout analysis)
    let no_boxes_flow_opts = ExtractOptions {
        laparams: Some(LAParams {
            boxes_flow: None,
            ..Default::default()
        }),
        ..Default::default()
    };

    group.bench_with_input(
        BenchmarkId::from_parameter("no_boxes_flow"),
        &data,
        |b, data| {
            b.iter(|| {
                extract_text(black_box(data), Some(no_boxes_flow_opts.clone()))
                    .expect("Failed to extract text")
            })
        },
    );

    // Detect vertical text
    let detect_vertical_opts = ExtractOptions {
        laparams: Some(LAParams {
            detect_vertical: true,
            ..Default::default()
        }),
        ..Default::default()
    };

    group.bench_with_input(
        BenchmarkId::from_parameter("detect_vertical"),
        &data,
        |b, data| {
            b.iter(|| {
                extract_text(black_box(data), Some(detect_vertical_opts.clone()))
                    .expect("Failed to extract text")
            })
        },
    );

    // Also test with jo.pdf (Japanese) which may benefit from detect_vertical
    let jo_data = load_fixture("jo.pdf");

    group.bench_with_input(
        BenchmarkId::from_parameter("jo_default"),
        &jo_data,
        |b, data| {
            b.iter(|| {
                extract_text(black_box(data), Some(default_opts.clone()))
                    .expect("Failed to extract text")
            })
        },
    );

    group.bench_with_input(
        BenchmarkId::from_parameter("jo_detect_vertical"),
        &jo_data,
        |b, data| {
            b.iter(|| {
                extract_text(black_box(data), Some(detect_vertical_opts.clone()))
                    .expect("Failed to extract text")
            })
        },
    );

    group.finish();
}

/// Benchmark extract_pages - iterate over LTPage objects.
///
/// Tests the page iterator API which returns LTPage objects for layout inspection.
fn bench_e2e_extract_pages(c: &mut Criterion) {
    let mut group = c.benchmark_group("e2e_extract_pages");

    let fixtures = [
        ("simple1", "simple1.pdf"),
        ("simple4", "simple4.pdf"),
        ("simple5", "simple5.pdf"),
        ("jo", "jo.pdf"),
    ];

    for (name, filename) in fixtures {
        let data = load_fixture(filename);

        group.bench_with_input(BenchmarkId::from_parameter(name), &data, |b, data| {
            b.iter(|| {
                let pages: Vec<_> = extract_pages(black_box(data), None)
                    .expect("Failed to extract pages")
                    .filter_map(|p| p.ok())
                    .collect();
                pages.len()
            })
        });
    }

    group.finish();
}

/// Benchmark first page extraction - maxpages=1 for quick extraction.
///
/// Tests the common use case of extracting just the first page for preview
/// or quick text extraction.
fn bench_e2e_first_page(c: &mut Criterion) {
    let mut group = c.benchmark_group("e2e_first_page");

    let fixtures = [
        ("simple1", "simple1.pdf"),
        ("simple4", "simple4.pdf"),
        ("simple5", "simple5.pdf"),
        ("jo", "jo.pdf"),
    ];

    let first_page_opts = ExtractOptions {
        maxpages: 1,
        ..Default::default()
    };

    for (name, filename) in fixtures {
        let data = load_fixture(filename);

        group.bench_with_input(BenchmarkId::from_parameter(name), &data, |b, data| {
            b.iter(|| {
                extract_text(black_box(data), Some(first_page_opts.clone()))
                    .expect("Failed to extract text")
            })
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_e2e_extract_text,
    bench_e2e_laparams,
    bench_e2e_extract_pages,
    bench_e2e_first_page
);
criterion_main!(benches);
