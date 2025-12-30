//! Benchmarks for layout analysis algorithms.
//!
//! These benchmarks target the O(n^2) layout algorithms in `src/layout.rs`:
//! - `group_objects`: Characters to text lines
//! - `group_textlines`: Text lines to text boxes
//! - `group_textboxes`: Text boxes to hierarchical groups

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;

use bolivar_core::layout::{LAParams, LTChar, LTLayoutContainer, TextBoxType, TextLineType};

/// US Letter page bounding box (width=612pt, height=792pt)
const PAGE_BBOX: (f64, f64, f64, f64) = (0.0, 0.0, 612.0, 792.0);

// =============================================================================
// Data Generation
// =============================================================================

/// Generate characters in a grid layout (typical PDF page).
///
/// Simulates text on a US Letter page with:
/// - chars_per_line characters per line
/// - num_lines lines of text
/// - Standard 12pt font size
fn generate_grid_chars(chars_per_line: usize, num_lines: usize) -> Vec<LTChar> {
    let char_width = 7.2; // Typical 12pt font character width
    let char_height = 12.0;
    let line_spacing = 14.4; // 1.2x line height
    let left_margin = 72.0; // 1 inch margin
    let top_margin = 720.0; // Start from top of US Letter page

    let mut chars = Vec::with_capacity(chars_per_line * num_lines);

    for line_idx in 0..num_lines {
        let y0 = top_margin - (line_idx as f64 * line_spacing) - char_height;
        let y1 = y0 + char_height;

        for char_idx in 0..chars_per_line {
            let x0 = left_margin + (char_idx as f64 * char_width);
            let x1 = x0 + char_width;

            // Use 'a' for most chars, space every ~10 chars to create words
            let text = if char_idx % 10 == 9 { " " } else { "a" };

            chars.push(LTChar::new(
                (x0, y0, x1, y1),
                text,
                "Helvetica",
                12.0,
                true, // upright
                char_width,
            ));
        }
    }

    chars
}

/// Generate characters with random overlapping bboxes (adversarial case).
///
/// Creates characters scattered across the page with intentional overlaps
/// to stress spatial algorithms.
fn generate_adversarial_chars(n: usize) -> Vec<LTChar> {
    let page_width = 612.0;
    let page_height = 792.0;
    let char_height = 12.0;

    // Use a simple PRNG for reproducibility (no external dependency)
    let mut seed: u64 = 12345;
    let mut rand = || {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        ((seed >> 16) & 0x7FFF) as f64 / 32768.0
    };

    let mut chars = Vec::with_capacity(n);

    for _ in 0..n {
        // Random position with varying widths
        let x0 = rand() * (page_width - 100.0);
        let width = 5.0 + rand() * 50.0; // Variable width 5-55 points
        let x1 = (x0 + width).min(page_width);

        let y0 = rand() * (page_height - char_height);
        let y1 = y0 + char_height;

        chars.push(LTChar::new(
            (x0, y0, x1, y1),
            "x",
            "Helvetica",
            12.0,
            true,
            width,
        ));
    }

    chars
}

/// Generate text lines by running chars through group_objects.
///
/// This produces realistic lines by using the actual grouping algorithm.
fn generate_lines_via_pipeline(
    chars_per_line: usize,
    num_lines: usize,
) -> (LTLayoutContainer, LAParams, Vec<TextLineType>) {
    let laparams = LAParams::default();
    let container = LTLayoutContainer::new(PAGE_BBOX);
    let chars = generate_grid_chars(chars_per_line, num_lines);
    let lines = container.group_objects(&laparams, &chars);
    (container, laparams, lines)
}

/// Generate N independent text boxes using multi-column layout.
///
/// Creates boxes that won't be grouped together by using different
/// x-positions (columns) and/or varying line heights.
fn generate_independent_boxes(n: usize) -> (LTLayoutContainer, LAParams, Vec<TextBoxType>) {
    let laparams = LAParams::default();
    let container = LTLayoutContainer::new(PAGE_BBOX);

    // Create boxes as independent paragraphs in a multi-column layout
    // Each "paragraph" has lines with different heights/alignments to prevent grouping
    let columns = 3;
    let column_width = 180.0;
    let box_height = 40.0;
    let vertical_gap = 20.0;

    let mut all_chars = Vec::new();

    for i in 0..n {
        let col = i % columns;
        let row = i / columns;

        // Vary the left margin per column to prevent left-alignment grouping
        let left_margin = 36.0 + (col as f64 * (column_width + 20.0)) + (row as f64 * 3.0);
        let top = 720.0 - (row as f64 * (box_height + vertical_gap));

        // Vary line height to prevent same-height grouping
        let line_height = 10.0 + (i % 3) as f64 * 2.0;
        let char_width = 6.0 + (i % 4) as f64;

        // Create 2-3 lines per box, but with varying properties
        let lines_in_box = 2 + (i % 2);
        for line_idx in 0..lines_in_box {
            let y0 = top - ((line_idx + 1) as f64 * (line_height + 2.0));
            let y1 = y0 + line_height;

            // 5-10 chars per line
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

    // Run through the pipeline
    let lines = container.group_objects(&laparams, &all_chars);
    let boxes = container.group_textlines(&laparams, lines);
    (container, laparams, boxes)
}

/// Generate adversarial lines by running random chars through group_objects.
fn generate_adversarial_lines_via_pipeline(
    n: usize,
) -> (LTLayoutContainer, LAParams, Vec<TextLineType>) {
    let laparams = LAParams::default();
    let container = LTLayoutContainer::new(PAGE_BBOX);
    let chars = generate_adversarial_chars(n);
    let lines = container.group_objects(&laparams, &chars);
    (container, laparams, lines)
}

/// Generate adversarial boxes by running random lines through the pipeline.
fn generate_adversarial_boxes_via_pipeline(
    n: usize,
) -> (LTLayoutContainer, LAParams, Vec<TextBoxType>) {
    let (container, laparams, lines) = generate_adversarial_lines_via_pipeline(n);
    let boxes = container.group_textlines(&laparams, lines);
    (container, laparams, boxes)
}

// =============================================================================
// Benchmark Groups
// =============================================================================

/// Benchmark group_objects: characters to text lines
fn bench_group_objects(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_group_objects");
    let laparams = LAParams::default();

    // Grid layout: typical page with 80 chars/line
    for n in [100usize, 500, 1_000, 5_000] {
        let lines_count = n.div_ceil(80); // ~80 chars per line
        let chars = generate_grid_chars(80.min(n), lines_count.max(1));

        group.bench_with_input(BenchmarkId::new("grid", n), &chars, |b, chars| {
            let container = LTLayoutContainer::new(PAGE_BBOX);
            b.iter(|| container.group_objects(black_box(&laparams), black_box(chars)))
        });
    }

    group.finish();
}

/// Benchmark group_textlines: text lines to text boxes
fn bench_group_textlines(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_group_textlines");

    // Generate lines with realistic content by running through the pipeline
    // Target line counts: 10, 50, 100, 500
    // Each line has ~80 chars, so we generate enough chars
    for target_lines in [10, 50, 100, 500] {
        // Generate extra chars to produce approximately target_lines lines
        let (container, laparams, lines) = generate_lines_via_pipeline(80, target_lines);
        let actual_lines = lines.len();

        group.bench_with_input(
            BenchmarkId::new("grid", actual_lines),
            &lines,
            |b, lines| {
                b.iter_batched(
                    || lines.clone(),
                    |lines| container.group_textlines(black_box(&laparams), black_box(lines)),
                    BatchSize::SmallInput,
                )
            },
        );
    }

    group.finish();
}

/// Benchmark group_textboxes: text boxes to hierarchical groups
///
/// CAPPED at 200 boxes due to O(n^2) complexity (200 boxes = 20K pairs)
fn bench_group_textboxes(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_group_textboxes");

    // Generate independent boxes using multi-column layout
    // This ensures we get approximately the target number of boxes
    for target_boxes in [10, 50, 100, 200] {
        let (container, laparams, boxes) = generate_independent_boxes(target_boxes);
        let actual_boxes = boxes.len();

        // Skip if we got too few boxes
        if actual_boxes < 5 {
            continue;
        }

        group.bench_with_input(
            BenchmarkId::new("grid", actual_boxes),
            &boxes,
            |b, boxes| b.iter(|| container.group_textboxes(black_box(&laparams), black_box(boxes))),
        );
    }

    group.finish();
}

/// Benchmark full pipeline: chars -> lines -> boxes
fn bench_full_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_full_pipeline");
    let laparams = LAParams::default();

    // Test with typical page sizes
    for (chars_per_line, num_lines) in [(80, 10), (80, 25), (80, 50)] {
        let n = chars_per_line * num_lines;
        let chars = generate_grid_chars(chars_per_line, num_lines);

        group.bench_with_input(BenchmarkId::new("grid", n), &chars, |b, chars| {
            let container = LTLayoutContainer::new(PAGE_BBOX);
            b.iter(|| {
                // Full pipeline: chars -> lines -> boxes
                let lines = container.group_objects(black_box(&laparams), black_box(chars));
                let boxes = container.group_textlines(black_box(&laparams), lines);
                container.group_textboxes(black_box(&laparams), &boxes)
            })
        });
    }

    group.finish();
}

/// Benchmark adversarial cases: overlapping elements stress test
fn bench_adversarial(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_adversarial");
    let laparams = LAParams::default();

    // Adversarial chars (random overlapping positions)
    for n in [100, 500, 1_000] {
        let chars = generate_adversarial_chars(n);

        group.bench_with_input(BenchmarkId::new("chars", n), &chars, |b, chars| {
            let container = LTLayoutContainer::new(PAGE_BBOX);
            b.iter(|| container.group_objects(black_box(&laparams), black_box(chars)))
        });
    }

    // Adversarial lines (random overlapping positions)
    for n in [100, 500, 1_000] {
        let (container, laparams, lines) = generate_adversarial_lines_via_pipeline(n);
        let actual_lines = lines.len();

        // Skip if we got too few lines
        if actual_lines < 5 {
            continue;
        }

        group.bench_with_input(
            BenchmarkId::new("lines", actual_lines),
            &lines,
            |b, lines| {
                b.iter_batched(
                    || lines.clone(),
                    |lines| container.group_textlines(black_box(&laparams), black_box(lines)),
                    BatchSize::SmallInput,
                )
            },
        );
    }

    // Adversarial boxes (random overlapping positions) - capped at 100
    for n in [100, 500, 1_000] {
        let (container, laparams, boxes) = generate_adversarial_boxes_via_pipeline(n);
        let actual_boxes = boxes.len();

        // Skip if we got too few boxes, or cap at 100 for O(n^2) safety
        if !(5..=100).contains(&actual_boxes) {
            continue;
        }

        group.bench_with_input(
            BenchmarkId::new("boxes", actual_boxes),
            &boxes,
            |b, boxes| b.iter(|| container.group_textboxes(black_box(&laparams), black_box(boxes))),
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_group_objects,
    bench_group_textlines,
    bench_group_textboxes,
    bench_full_pipeline,
    bench_adversarial
);
criterion_main!(benches);
