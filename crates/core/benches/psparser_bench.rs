//! Benchmarks for PostScript tokenization.
//!
//! These benchmarks target `PSBaseParser::next_token()` - the core tokenization
//! function that processes all PDF content streams.
//!
//! Benchmark groups:
//! - `psparser_tokenize`: Raw tokenization throughput at various scales
//! - `psparser_token_types`: Isolated benchmarks for specific token types
//! - `psparser_cmap`: CMap-shaped data (begincmap blocks, integer ranges)

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;

use bolivar_core::psparser::PSBaseParser;

// =============================================================================
// Data Generation
// =============================================================================

/// Generate synthetic PostScript data with N tokens.
///
/// Produces a mix of common PDF content stream tokens:
/// - Integers and reals (coordinates, font sizes)
/// - Literal names (/Name)
/// - Keywords (BT, ET, Tj, Tm, etc.)
/// - Strings (literal and hex)
fn generate_mixed_tokens(n: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(n * 15); // ~15 bytes avg per token

    // Token templates cycling through common PDF operations
    let templates: &[&[u8]] = &[
        b"BT ",             // Begin text
        b"/F1 ",            // Font name
        b"12 ",             // Font size
        b"Tf ",             // Set font
        b"100 ",            // X coordinate
        b"700 ",            // Y coordinate
        b"Td ",             // Move text position
        b"(Hello World) ",  // String
        b"Tj ",             // Show text
        b"ET ",             // End text
        b"0.5 ",            // Real number
        b"<48454C4C4F> ",   // Hex string
        b"/DeviceRGB ",     // Color space
        b"cs ",             // Set color space
        b"0 0 0 ",          // RGB values (3 tokens)
        b"sc ",             // Set color
        b"q ",              // Save graphics state
        b"Q ",              // Restore graphics state
        b"1 0 0 1 72 720 ", // Matrix values (6 tokens)
        b"cm ",             // Concat matrix
    ];

    let mut i = 0;
    while i < n {
        let template = templates[i % templates.len()];
        data.extend_from_slice(template);
        i += 1;
    }

    data
}

/// Generate data with primarily integer tokens.
fn generate_integer_tokens(n: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(n * 6);

    for i in 0..n {
        // Mix of positive and negative, small and large integers
        let value = match i % 4 {
            0 => format!("{} ", i % 1000),
            1 => format!("-{} ", (i % 500)),
            2 => format!("{} ", (i % 10000) * 100),
            _ => format!("{} ", i % 100),
        };
        data.extend_from_slice(value.as_bytes());
    }

    data
}

/// Generate data with primarily real (floating point) tokens.
fn generate_real_tokens(n: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(n * 10);

    for i in 0..n {
        let value = match i % 6 {
            0 => format!("{}.{} ", i % 100, (i * 7) % 100),
            1 => format!("-{}.{} ", i % 50, (i * 3) % 100),
            2 => format!("0.{:03} ", i % 1000),
            3 => format!(".{} ", (i % 99) + 1),
            4 => format!("{}. ", i % 100),       // trailing dot "123."
            _ => format!("-.{} ", (i % 99) + 1), // negative no leading zero "-.5"
        };
        data.extend_from_slice(value.as_bytes());
    }

    data
}

/// Generate data with literal string tokens.
fn generate_string_tokens(n: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(n * 20);

    // Various string patterns
    let strings: &[&[u8]] = &[
        b"(Hello) ",
        b"(World) ",
        b"(Test String) ",
        b"(PDF Content) ",
        b"(Line 1\\nLine 2) ",      // Escape sequence
        b"(Nested (parens) here) ", // Nested parentheses
        b"(Tab\\there) ",           // Tab escape
        b"(Octal\\101\\102\\103) ", // Octal escapes (ABC)
        b"(Simple) ",
        b"() ", // Empty string
    ];

    for i in 0..n {
        data.extend_from_slice(strings[i % strings.len()]);
    }

    data
}

/// Generate data with hex string tokens.
fn generate_hex_string_tokens(n: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(n * 20);

    // Various hex string patterns
    let hex_strings: &[&[u8]] = &[
        b"<48454C4C4F> ",          // "HELLO"
        b"<576F726C64> ",          // "World"
        b"<00FF00FF> ",            // Binary data
        b"<DEADBEEF> ",            // Classic test pattern
        b"<0123456789ABCDEF> ",    // All hex digits
        b"<> ",                    // Empty hex string
        b"<4 8 4 5 4 C 4 C 4 F> ", // With whitespace
        b"<CAFEBABE> ",            // Another test pattern
    ];

    for i in 0..n {
        data.extend_from_slice(hex_strings[i % hex_strings.len()]);
    }

    data
}

/// Generate CMap-shaped PostScript data.
///
/// CMap files define character mappings and are commonly embedded in PDFs.
/// They have a specific structure with:
/// - begincmap/endcmap blocks
/// - begincodespacerange/endcodespacerange
/// - beginbfchar/endbfchar
/// - beginbfrange/endbfrange
/// - Integer ranges and hex string mappings
fn generate_cmap_data(entries: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(entries * 50);

    // CMap header
    data.extend_from_slice(b"/CIDInit /ProcSet findresource begin\n");
    data.extend_from_slice(b"12 dict begin\n");
    data.extend_from_slice(b"begincmap\n");
    data.extend_from_slice(b"/CIDSystemInfo <<\n");
    data.extend_from_slice(b"  /Registry (Adobe)\n");
    data.extend_from_slice(b"  /Ordering (UCS)\n");
    data.extend_from_slice(b"  /Supplement 0\n");
    data.extend_from_slice(b">> def\n");
    data.extend_from_slice(b"/CMapName /Adobe-Identity-UCS def\n");
    data.extend_from_slice(b"/CMapType 2 def\n");

    // Code space range
    data.extend_from_slice(b"1 begincodespacerange\n");
    data.extend_from_slice(b"<0000> <FFFF>\n");
    data.extend_from_slice(b"endcodespacerange\n");

    // Generate bfchar entries (single character mappings)
    let bfchar_count = entries / 2;
    if bfchar_count > 0 {
        data.extend_from_slice(format!("{} beginbfchar\n", bfchar_count).as_bytes());
        for i in 0..bfchar_count {
            // Map CID to Unicode
            let cid = i + 1;
            let unicode = 0x0041 + (i % 26); // A-Z cycling
            data.extend_from_slice(format!("<{:04X}> <{:04X}>\n", cid, unicode).as_bytes());
        }
        data.extend_from_slice(b"endbfchar\n");
    }

    // Generate bfrange entries (range mappings)
    let bfrange_count = entries / 2;
    if bfrange_count > 0 {
        data.extend_from_slice(format!("{} beginbfrange\n", bfrange_count).as_bytes());
        for i in 0..bfrange_count {
            // Range of CIDs to Unicode range
            let start_cid = 0x1000 + (i * 16);
            let end_cid = start_cid + 15;
            let start_unicode = 0x4E00 + (i * 16); // CJK Unified Ideographs
            data.extend_from_slice(
                format!(
                    "<{:04X}> <{:04X}> <{:04X}>\n",
                    start_cid, end_cid, start_unicode
                )
                .as_bytes(),
            );
        }
        data.extend_from_slice(b"endbfrange\n");
    }

    // CMap footer
    data.extend_from_slice(b"endcmap\n");
    data.extend_from_slice(b"CMapName currentdict /CMap defineresource pop\n");
    data.extend_from_slice(b"end\n");
    data.extend_from_slice(b"end\n");

    data
}

/// Count tokens in data (for verification and reporting).
fn count_tokens(data: &[u8]) -> usize {
    let mut parser = PSBaseParser::new(data);
    let mut count = 0;
    while parser.next_token().is_some() {
        count += 1;
    }
    count
}

// =============================================================================
// Benchmark Groups
// =============================================================================

/// Benchmark raw tokenization throughput at various scales.
///
/// Tests with 10K, 100K, and 1M tokens to measure throughput
/// at scales that drown out measurement overhead.
fn bench_tokenize(c: &mut Criterion) {
    let mut group = c.benchmark_group("psparser_tokenize");

    for target_tokens in [10_000usize, 100_000, 1_000_000] {
        let data = generate_mixed_tokens(target_tokens);
        let actual_tokens = count_tokens(&data);

        group.bench_with_input(
            BenchmarkId::new("mixed", actual_tokens),
            &data,
            |b, data| {
                b.iter(|| {
                    let mut parser = PSBaseParser::new(black_box(data));
                    let mut count = 0usize;
                    while let Some(result) = parser.next_token() {
                        black_box(result.unwrap());
                        count += 1;
                    }
                    count
                })
            },
        );
    }

    group.finish();
}

/// Benchmark specific token types in isolation.
///
/// Measures parsing performance for:
/// - Integers (most common in coordinate data)
/// - Reals (floating point numbers)
/// - Literal strings (text content)
/// - Hex strings (binary data, CMap mappings)
fn bench_token_types(c: &mut Criterion) {
    let mut group = c.benchmark_group("psparser_token_types");

    // Use 100K tokens for all type-specific benchmarks
    let n = 100_000;

    // Integer tokens
    {
        let data = generate_integer_tokens(n);
        let actual = count_tokens(&data);
        group.bench_with_input(BenchmarkId::new("integers", actual), &data, |b, data| {
            b.iter(|| {
                let mut parser = PSBaseParser::new(black_box(data));
                while let Some(result) = parser.next_token() {
                    black_box(result.unwrap());
                }
            })
        });
    }

    // Real tokens
    {
        let data = generate_real_tokens(n);
        let actual = count_tokens(&data);
        group.bench_with_input(BenchmarkId::new("reals", actual), &data, |b, data| {
            b.iter(|| {
                let mut parser = PSBaseParser::new(black_box(data));
                while let Some(result) = parser.next_token() {
                    black_box(result.unwrap());
                }
            })
        });
    }

    // Literal string tokens
    {
        let data = generate_string_tokens(n);
        let actual = count_tokens(&data);
        group.bench_with_input(BenchmarkId::new("strings", actual), &data, |b, data| {
            b.iter(|| {
                let mut parser = PSBaseParser::new(black_box(data));
                while let Some(result) = parser.next_token() {
                    black_box(result.unwrap());
                }
            })
        });
    }

    // Hex string tokens
    {
        let data = generate_hex_string_tokens(n);
        let actual = count_tokens(&data);
        group.bench_with_input(BenchmarkId::new("hex_strings", actual), &data, |b, data| {
            b.iter(|| {
                let mut parser = PSBaseParser::new(black_box(data));
                while let Some(result) = parser.next_token() {
                    black_box(result.unwrap());
                }
            })
        });
    }

    group.finish();
}

/// Benchmark CMap-shaped PostScript data.
///
/// CMap files are commonly embedded in PDFs for character mapping.
/// They have a distinctive structure with:
/// - begincmap/endcmap blocks
/// - Integer ranges (beginbfrange)
/// - Hex string mappings
fn bench_cmap(c: &mut Criterion) {
    let mut group = c.benchmark_group("psparser_cmap");

    // Various CMap sizes (number of mapping entries)
    for entries in [100, 500, 1_000, 5_000] {
        let data = generate_cmap_data(entries);
        let actual_tokens = count_tokens(&data);

        group.bench_with_input(BenchmarkId::new("entries", entries), &data, |b, data| {
            b.iter(|| {
                let mut parser = PSBaseParser::new(black_box(data));
                let mut count = 0usize;
                while let Some(result) = parser.next_token() {
                    black_box(result.unwrap());
                    count += 1;
                }
                count
            })
        });

        // Also report tokens per entry ratio (useful for understanding CMap complexity)
        if entries == 100 {
            // Just log once
            eprintln!(
                "CMap with {} entries produces {} tokens ({:.1} tokens/entry)",
                entries,
                actual_tokens,
                actual_tokens as f64 / entries as f64
            );
        }
    }

    group.finish();
}

criterion_group!(benches, bench_tokenize, bench_token_types, bench_cmap);
criterion_main!(benches);
