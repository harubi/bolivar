//! Benchmarks for PDF document parsing and object resolution.
//!
//! These benchmarks target the core document operations in `src/pdfdocument.rs`:
//! - `PDFDocument::new`: Document initialization and xref parsing
//! - `getobj` / `resolve`: Object lookup and resolution
//! - Page iteration via `PDFPage::create_pages`

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;

use bolivar_core::pdfdocument::PDFDocument;
use bolivar_core::pdfpage::PDFPage;

/// Load a test fixture by name.
fn load_fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("Failed to load fixture {}: {}", path, e))
}

// =============================================================================
// Benchmark Groups
// =============================================================================

/// Benchmark PDFDocument::new - document initialization and xref parsing.
///
/// Tests initialization with various PDF sizes and complexities:
/// - simple1.pdf: Minimal PDF
/// - simple4.pdf: Slightly more complex
/// - simple5.pdf: Multi-page
/// - jo.pdf: Japanese text with CID fonts
fn bench_pdfdocument_init(c: &mut Criterion) {
    let mut group = c.benchmark_group("pdfdocument_init");

    let fixtures = [
        ("simple1", "simple1.pdf"),
        ("simple4", "simple4.pdf"),
        ("simple5", "simple5.pdf"),
        ("jo", "jo.pdf"),
    ];

    for (name, filename) in fixtures {
        let data = load_fixture(filename);

        group.bench_with_input(BenchmarkId::from_parameter(name), &data, |b, data| {
            b.iter(|| PDFDocument::new(black_box(data), "").expect("Failed to parse PDF"))
        });
    }

    group.finish();
}

/// Benchmark object resolution - resolve all objects in a document.
///
/// Uses simple4.pdf which has a reasonable number of objects to resolve.
fn bench_pdfdocument_resolve(c: &mut Criterion) {
    let mut group = c.benchmark_group("pdfdocument_resolve");

    let data = load_fixture("simple4.pdf");
    let doc = PDFDocument::new(&data, "").expect("Failed to parse PDF");
    let objids = doc.get_objids();

    group.bench_function("simple4_all_objects", |b| {
        b.iter(|| {
            for &objid in &objids {
                if let Ok(obj) = doc.getobj(black_box(objid)) {
                    let _ = doc.resolve(black_box(&obj));
                }
            }
        })
    });

    // Also benchmark resolving a single object repeatedly
    if let Some(&first_objid) = objids.first() {
        group.bench_function("simple4_single_object", |b| {
            b.iter(|| {
                let obj = doc
                    .getobj(black_box(first_objid))
                    .expect("object should exist");
                doc.resolve(black_box(&obj))
            })
        });
    }

    group.finish();
}

/// Benchmark page iteration - iterate through all pages in a document.
///
/// Tests both single-page and multi-page documents.
fn bench_pdfdocument_pages(c: &mut Criterion) {
    let mut group = c.benchmark_group("pdfdocument_pages");

    let fixtures = [
        ("simple1", "simple1.pdf"),
        ("simple4", "simple4.pdf"),
        ("simple5", "simple5.pdf"),
        ("jo", "jo.pdf"),
    ];

    for (name, filename) in fixtures {
        let data = load_fixture(filename);
        let doc = PDFDocument::new(&data, "").expect("Failed to parse PDF");

        group.bench_with_input(BenchmarkId::from_parameter(name), &doc, |b, doc| {
            b.iter(|| {
                let pages: Vec<_> = PDFPage::create_pages(black_box(doc))
                    .filter_map(|p| p.ok())
                    .collect();
                pages.len()
            })
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_pdfdocument_init,
    bench_pdfdocument_resolve,
    bench_pdfdocument_pages
);
criterion_main!(benches);
