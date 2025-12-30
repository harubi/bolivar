//! Benchmarks for PDF content stream parsing.
//!
//! These benchmarks target `PDFContentParser::next_with_pos()` which parses
//! content streams containing operators like BT, ET, Tm, Tj, and graphics ops.

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;

use bolivar::pdfinterp::PDFContentParser;

// =============================================================================
// Content Stream Generators
// =============================================================================

/// Generate a content stream with text operations.
///
/// Creates realistic text content with:
/// - BT/ET blocks (begin/end text)
/// - Tm (text matrix) with numeric operands
/// - Tj (show text) with string operands
/// - Tf (set font) with name and number
fn generate_text_ops(n: usize) -> Vec<u8> {
    let mut stream = Vec::with_capacity(n * 100);

    for i in 0..n {
        // Begin text block
        stream.extend_from_slice(b"BT\n");

        // Set font: /F1 12 Tf
        stream.extend_from_slice(b"/F1 12 Tf\n");

        // Text matrix: 1 0 0 1 x y Tm
        let x = (i % 10) * 60 + 72;
        let y = 720 - (i / 10) * 14;
        stream.extend_from_slice(format!("1 0 0 1 {} {} Tm\n", x, y).as_bytes());

        // Show text: (Hello World) Tj
        stream.extend_from_slice(b"(Hello World) Tj\n");

        // End text block
        stream.extend_from_slice(b"ET\n");
    }

    stream
}

/// Generate a content stream with graphics operations.
///
/// Creates graphics state operators:
/// - q/Q (save/restore graphics state)
/// - cm (concat matrix)
/// - m/l/c (moveto, lineto, curveto)
/// - S/f (stroke, fill)
/// - re (rectangle)
/// - rg/RG (set color)
fn generate_graphics_ops(n: usize) -> Vec<u8> {
    let mut stream = Vec::with_capacity(n * 150);

    for i in 0..n {
        // Save graphics state
        stream.extend_from_slice(b"q\n");

        // Concat matrix
        let scale = 1.0 + (i % 10) as f64 * 0.1;
        stream.extend_from_slice(format!("{:.2} 0 0 {:.2} 0 0 cm\n", scale, scale).as_bytes());

        // Set stroke color
        stream.extend_from_slice(b"0.5 0.5 0.5 RG\n");

        // Set fill color
        stream.extend_from_slice(b"0.8 0.8 0.8 rg\n");

        // Draw rectangle
        let x = (i % 10) * 50 + 72;
        let y = 720 - (i / 10) * 50;
        stream.extend_from_slice(format!("{} {} 40 30 re\n", x, y).as_bytes());

        // Fill and stroke
        stream.extend_from_slice(b"B\n");

        // Draw a path
        stream.extend_from_slice(format!("{} {} m\n", x, y).as_bytes());
        stream.extend_from_slice(format!("{} {} l\n", x + 40, y + 30).as_bytes());
        stream.extend_from_slice(
            format!(
                "{} {} {} {} {} {} c\n",
                x + 20,
                y,
                x + 40,
                y + 15,
                x + 40,
                y + 30
            )
            .as_bytes(),
        );
        stream.extend_from_slice(b"S\n");

        // Restore graphics state
        stream.extend_from_slice(b"Q\n");
    }

    stream
}

/// Generate a content stream with mixed text and graphics.
fn generate_mixed_ops(n: usize) -> Vec<u8> {
    let text_count = n / 2;
    let graphics_count = n - text_count;

    let mut stream = Vec::with_capacity(n * 120);

    // Interleave text and graphics
    for i in 0..n.max(1) {
        if i % 2 == 0 && i / 2 < text_count {
            // Text operation
            stream.extend_from_slice(b"BT\n");
            stream.extend_from_slice(b"/F1 12 Tf\n");
            let x = (i % 10) * 60 + 72;
            let y = 720 - (i / 10) * 14;
            stream.extend_from_slice(format!("1 0 0 1 {} {} Tm\n", x, y).as_bytes());
            stream.extend_from_slice(b"(Sample text) Tj\n");
            stream.extend_from_slice(b"ET\n");
        } else if i / 2 < graphics_count {
            // Graphics operation
            stream.extend_from_slice(b"q\n");
            stream.extend_from_slice(b"0.5 0.5 0.5 rg\n");
            let x = (i % 10) * 50 + 72;
            let y = 720 - (i / 10) * 50;
            stream.extend_from_slice(format!("{} {} 30 20 re f\n", x, y).as_bytes());
            stream.extend_from_slice(b"Q\n");
        }
    }

    stream
}

/// Generate a content stream with nested arrays and dictionaries.
///
/// Creates stress test for array/dict parsing:
/// - Nested arrays [[1 2] [3 4]]
/// - Inline dictionaries <<...>>
/// - Deep nesting
fn generate_nested_structures(depth: usize, width: usize) -> Vec<u8> {
    let mut stream = Vec::with_capacity(depth * width * 50);

    // Generate nested arrays with operators
    for _ in 0..width {
        // Open nested arrays
        for _ in 0..depth {
            stream.extend_from_slice(b"[");
        }

        // Inner content
        stream.extend_from_slice(b"1 2 3 4 5");

        // Close nested arrays
        for _ in 0..depth {
            stream.extend_from_slice(b"]");
        }

        // Use the array in setdash operator (array phase d)
        stream.extend_from_slice(b" 0 d\n");
    }

    // Generate nested dictionaries
    for _ in 0..width {
        // Open nested dicts
        for d in 0..depth {
            stream.extend_from_slice(b"<<");
            stream.extend_from_slice(format!("/Level{} ", d).as_bytes());
        }

        // Inner value
        stream.extend_from_slice(b"42");

        // Close nested dicts
        for _ in 0..depth {
            stream.extend_from_slice(b">>");
        }

        // Dict parsed as operand; use no-op graphics state save/restore
        stream.extend_from_slice(b" q Q\n");
    }

    stream
}

/// Generate deeply nested mixed structures.
fn generate_deeply_nested(n: usize) -> Vec<u8> {
    let mut stream = Vec::with_capacity(n * 200);

    for i in 0..n {
        let depth = 1 + (i % 5); // Vary depth 1-5

        // Array containing dicts containing arrays
        for _ in 0..depth {
            stream.extend_from_slice(b"[");
        }

        stream.extend_from_slice(b"<</Type /Test /Data ");

        for _ in 0..depth {
            stream.extend_from_slice(b"[");
        }

        stream.extend_from_slice(format!("{} {} {}", i, i + 1, i + 2).as_bytes());

        for _ in 0..depth {
            stream.extend_from_slice(b"]");
        }

        stream.extend_from_slice(b">>");

        for _ in 0..depth {
            stream.extend_from_slice(b"]");
        }

        // Use outer array in setdash
        stream.extend_from_slice(b" 0 d\n");
    }

    stream
}

/// Generate a content stream with inline images.
///
/// Creates BI/ID/EI (begin inline image, image data, end inline image)
/// sequences which are the most complex PDF content stream parsing case.
fn generate_inline_images(n: usize) -> Vec<u8> {
    let mut stream = Vec::with_capacity(n * 100);
    for _ in 0..n {
        // BI <dict> ID <data> EI
        stream.extend_from_slice(b"BI /W 8 /H 8 /BPC 8 /CS /G ID ");
        stream.extend_from_slice(&[0u8; 64]); // 8x8 grayscale
        stream.extend_from_slice(b" EI\n");
    }
    stream
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Consume all tokens from a parser, counting them.
fn consume_all_tokens(parser: &mut PDFContentParser) -> usize {
    let mut count = 0;
    while parser.next_with_pos().is_some() {
        count += 1;
    }
    count
}

// =============================================================================
// Benchmark Groups
// =============================================================================

/// Benchmark content stream parsing with text and graphics operations.
fn bench_pdfinterp_content(c: &mut Criterion) {
    let mut group = c.benchmark_group("pdfinterp_content");

    // Text operations at different scales
    for n in [100, 1_000, 10_000] {
        let stream = generate_text_ops(n);

        group.bench_with_input(BenchmarkId::new("text_ops", n), &stream, |b, stream| {
            b.iter_batched(
                || PDFContentParser::new(vec![stream.clone()]),
                |mut parser| black_box(consume_all_tokens(&mut parser)),
                BatchSize::SmallInput,
            )
        });
    }

    // Graphics operations at different scales
    for n in [100, 1_000, 10_000] {
        let stream = generate_graphics_ops(n);

        group.bench_with_input(BenchmarkId::new("graphics_ops", n), &stream, |b, stream| {
            b.iter_batched(
                || PDFContentParser::new(vec![stream.clone()]),
                |mut parser| black_box(consume_all_tokens(&mut parser)),
                BatchSize::SmallInput,
            )
        });
    }

    // Mixed text and graphics
    for n in [100, 1_000, 10_000] {
        let stream = generate_mixed_ops(n);

        group.bench_with_input(BenchmarkId::new("mixed_ops", n), &stream, |b, stream| {
            b.iter_batched(
                || PDFContentParser::new(vec![stream.clone()]),
                |mut parser| black_box(consume_all_tokens(&mut parser)),
                BatchSize::SmallInput,
            )
        });
    }

    // Inline images (BI/ID/EI) - most complex parsing case
    for n in [100, 1_000] {
        let stream = generate_inline_images(n);

        group.bench_with_input(
            BenchmarkId::new("inline_images", n),
            &stream,
            |b, stream| {
                b.iter_batched(
                    || PDFContentParser::new(vec![stream.clone()]),
                    |mut parser| black_box(consume_all_tokens(&mut parser)),
                    BatchSize::SmallInput,
                )
            },
        );
    }

    group.finish();
}

/// Benchmark nested arrays/dicts stress test.
fn bench_pdfinterp_nested(c: &mut Criterion) {
    let mut group = c.benchmark_group("pdfinterp_nested");

    // Varying depth with fixed width
    for depth in [2, 5, 10] {
        let stream = generate_nested_structures(depth, 100);

        group.bench_with_input(BenchmarkId::new("depth", depth), &stream, |b, stream| {
            b.iter_batched(
                || PDFContentParser::new(vec![stream.clone()]),
                |mut parser| black_box(consume_all_tokens(&mut parser)),
                BatchSize::SmallInput,
            )
        });
    }

    // Varying width with fixed depth
    for width in [100, 500, 1_000] {
        let stream = generate_nested_structures(3, width);

        group.bench_with_input(BenchmarkId::new("width", width), &stream, |b, stream| {
            b.iter_batched(
                || PDFContentParser::new(vec![stream.clone()]),
                |mut parser| black_box(consume_all_tokens(&mut parser)),
                BatchSize::SmallInput,
            )
        });
    }

    // Deeply nested mixed structures
    for n in [100, 500, 1_000] {
        let stream = generate_deeply_nested(n);

        group.bench_with_input(
            BenchmarkId::new("deeply_nested", n),
            &stream,
            |b, stream| {
                b.iter_batched(
                    || PDFContentParser::new(vec![stream.clone()]),
                    |mut parser| black_box(consume_all_tokens(&mut parser)),
                    BatchSize::SmallInput,
                )
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_pdfinterp_content, bench_pdfinterp_nested);
criterion_main!(benches);
