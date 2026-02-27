use std::alloc::System;
use std::hint::black_box;
use stats_alloc::StatsAlloc;
use stats_alloc_helper::{LockedAllocator, memory_measured};
use bolivar_core::document::PDFDocument;
use bolivar_core::high_level::{ExtractOptions, extract_text_with_document};
use bolivar_core::layout::LAParams;

#[global_allocator]
static GLOBAL: LockedAllocator<System> = LockedAllocator::new(StatsAlloc::system());

fn main() {
    let fixture_path = "crates/core/tests/fixtures/simple1.pdf";
    let bytes = std::fs::read(fixture_path).expect("fixture not found");

    let doc = PDFDocument::new(&bytes, "").expect("parse PDF");
    let options = ExtractOptions {
        laparams: Some(LAParams::default()),
        ..Default::default()
    };

    let iters = 1;
    let stats = memory_measured(&GLOBAL, || {
        for _ in 0..iters {
            let text = extract_text_with_document(&doc, options.clone()).expect("extract text");
            black_box(text.len());
        }
    });

    println!(
        "alloc report: allocs={} deallocs={} bytes_allocated={} avg_allocs={:.2} avg_bytes={:.2}",
        stats.allocations,
        stats.deallocations,
        stats.bytes_allocated,
        stats.allocations as f64 / iters as f64,
        stats.bytes_allocated as f64 / iters as f64
    );
}
