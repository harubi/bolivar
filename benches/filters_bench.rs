//! Benchmarks for stream decoders (filters).
//!
//! These benchmarks target the critical hot path for compressed PDFs:
//! - `ascii85decode`: ASCII85 decoding
//! - `lzwdecode`: LZW decompression
//! - `rldecode`: RunLength decoding
//! - Chained filters: realistic multi-filter pipelines

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;

use bolivar::ascii85::ascii85decode;
use bolivar::lzw::lzwdecode;
use bolivar::runlength::rldecode;

// =============================================================================
// Test Data Generation
// =============================================================================

/// Generate raw bytes for testing (repeating pattern - compresses well).
fn generate_raw_bytes(size: usize) -> Vec<u8> {
    // Repeating pattern that compresses well
    (0..size).map(|i| (i % 256) as u8).collect()
}

/// Generate random bytes for testing (doesn't compress well).
/// Uses simple PRNG for reproducibility.
fn generate_random_bytes(size: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(size);
    let mut seed: u64 = 42;
    for _ in 0..size {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        data.push((seed >> 16) as u8);
    }
    data
}

/// Encode data to ASCII85 format.
///
/// ASCII85 encodes 4 bytes as 5 characters (bytes ! through u).
/// Special case: 4 zero bytes encode as 'z'.
fn ascii85_encode(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(data.len() * 5 / 4 + 10);
    result.extend_from_slice(b"<~");

    for chunk in data.chunks(4) {
        if chunk.len() == 4 {
            let value = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            if value == 0 {
                result.push(b'z');
            } else {
                let mut encoded = [0u8; 5];
                let mut v = value;
                for i in (0..5).rev() {
                    encoded[i] = (v % 85) as u8 + b'!';
                    v /= 85;
                }
                result.extend_from_slice(&encoded);
            }
        } else {
            // Pad with zeros for partial chunk
            let mut padded = [0u8; 4];
            padded[..chunk.len()].copy_from_slice(chunk);
            let value = u32::from_be_bytes(padded);
            let mut encoded = [0u8; 5];
            let mut v = value;
            for i in (0..5).rev() {
                encoded[i] = (v % 85) as u8 + b'!';
                v /= 85;
            }
            // Only output chunk.len() + 1 characters
            result.extend_from_slice(&encoded[..chunk.len() + 1]);
        }
    }

    result.extend_from_slice(b"~>");
    result
}

/// Encode data using LZW (PDF variant: MSB first, 8-bit).
///
/// Uses weezl crate for encoding (same as decoder uses).
fn lzw_encode(data: &[u8]) -> Vec<u8> {
    use weezl::{BitOrder, encode::Encoder};
    Encoder::new(BitOrder::Msb, 8)
        .encode(data)
        .expect("LZW encoding should succeed for benchmark data")
}

/// Encode data using RunLength encoding.
///
/// Format:
/// - Length byte 0-127: Copy next (length + 1) bytes literally
/// - Length byte 128: End of data (EOD marker)
/// - Length byte 129-255: Repeat next byte (257 - length) times
fn runlength_encode(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return vec![128]; // EOD
    }

    let mut result = Vec::new();
    let mut i = 0;

    while i < data.len() {
        // Look for runs of identical bytes
        let byte = data[i];
        let mut run_len = 1;
        while i + run_len < data.len() && data[i + run_len] == byte && run_len < 128 {
            run_len += 1;
        }

        if run_len >= 2 {
            // Encode as repeat run: length byte (257 - count), then the byte
            result.push((257 - run_len) as u8);
            result.push(byte);
            i += run_len;
        } else {
            // Collect literal run
            let start = i;
            let mut lit_len = 1;
            while i + lit_len < data.len() && lit_len < 128 {
                // Check if next bytes form a run of 2+
                let next_byte = data[i + lit_len];
                let next_run = if i + lit_len + 1 < data.len() && data[i + lit_len + 1] == next_byte
                {
                    2
                } else {
                    1
                };
                if next_run >= 2 {
                    break;
                }
                lit_len += 1;
            }
            // Encode as literal run: length byte (count - 1), then the bytes
            result.push((lit_len - 1) as u8);
            result.extend_from_slice(&data[start..start + lit_len]);
            i += lit_len;
        }
    }

    result.push(128); // EOD
    result
}

/// Pre-encoded test data for each filter at various sizes.
struct TestData {
    // Compressible (repeating pattern)
    ascii85_1k: Vec<u8>,
    ascii85_10k: Vec<u8>,
    ascii85_100k: Vec<u8>,
    lzw_1k: Vec<u8>,
    lzw_10k: Vec<u8>,
    lzw_100k: Vec<u8>,
    runlength_1k: Vec<u8>,
    runlength_10k: Vec<u8>,
    runlength_100k: Vec<u8>,
    // Random (doesn't compress well - realistic worst case)
    ascii85_random_1k: Vec<u8>,
    ascii85_random_10k: Vec<u8>,
    ascii85_random_100k: Vec<u8>,
    lzw_random_1k: Vec<u8>,
    lzw_random_10k: Vec<u8>,
    lzw_random_100k: Vec<u8>,
    runlength_random_1k: Vec<u8>,
    runlength_random_10k: Vec<u8>,
    runlength_random_100k: Vec<u8>,
}

impl TestData {
    fn new() -> Self {
        let raw_1k = generate_raw_bytes(1024);
        let raw_10k = generate_raw_bytes(10 * 1024);
        let raw_100k = generate_raw_bytes(100 * 1024);

        let random_1k = generate_random_bytes(1024);
        let random_10k = generate_random_bytes(10 * 1024);
        let random_100k = generate_random_bytes(100 * 1024);

        Self {
            ascii85_1k: ascii85_encode(&raw_1k),
            ascii85_10k: ascii85_encode(&raw_10k),
            ascii85_100k: ascii85_encode(&raw_100k),
            lzw_1k: lzw_encode(&raw_1k),
            lzw_10k: lzw_encode(&raw_10k),
            lzw_100k: lzw_encode(&raw_100k),
            runlength_1k: runlength_encode(&raw_1k),
            runlength_10k: runlength_encode(&raw_10k),
            runlength_100k: runlength_encode(&raw_100k),
            // Random data
            ascii85_random_1k: ascii85_encode(&random_1k),
            ascii85_random_10k: ascii85_encode(&random_10k),
            ascii85_random_100k: ascii85_encode(&random_100k),
            lzw_random_1k: lzw_encode(&random_1k),
            lzw_random_10k: lzw_encode(&random_10k),
            lzw_random_100k: lzw_encode(&random_100k),
            runlength_random_1k: runlength_encode(&random_1k),
            runlength_random_10k: runlength_encode(&random_10k),
            runlength_random_100k: runlength_encode(&random_100k),
        }
    }
}

// =============================================================================
// Benchmark Groups
// =============================================================================

/// Benchmark ASCII85 decoding at various sizes.
fn bench_ascii85(c: &mut Criterion) {
    let mut group = c.benchmark_group("filters_ascii85");
    let data = TestData::new();

    for (name, encoded) in [
        ("1K", &data.ascii85_1k),
        ("10K", &data.ascii85_10k),
        ("100K", &data.ascii85_100k),
    ] {
        group.bench_with_input(BenchmarkId::from_parameter(name), encoded, |b, encoded| {
            b.iter(|| ascii85decode(black_box(encoded)))
        });
    }

    group.finish();
}

/// Benchmark LZW decoding at various sizes.
fn bench_lzw(c: &mut Criterion) {
    let mut group = c.benchmark_group("filters_lzw");
    let data = TestData::new();

    for (name, encoded) in [
        ("1K", &data.lzw_1k),
        ("10K", &data.lzw_10k),
        ("100K", &data.lzw_100k),
    ] {
        group.bench_with_input(BenchmarkId::from_parameter(name), encoded, |b, encoded| {
            b.iter(|| lzwdecode(black_box(encoded)))
        });
    }

    group.finish();
}

/// Benchmark RunLength decoding at various sizes.
fn bench_runlength(c: &mut Criterion) {
    let mut group = c.benchmark_group("filters_runlength");
    let data = TestData::new();

    for (name, encoded) in [
        ("1K", &data.runlength_1k),
        ("10K", &data.runlength_10k),
        ("100K", &data.runlength_100k),
    ] {
        group.bench_with_input(BenchmarkId::from_parameter(name), encoded, |b, encoded| {
            b.iter(|| rldecode(black_box(encoded)))
        });
    }

    group.finish();
}

/// Benchmark realistic multi-filter pipelines.
///
/// PDFs often chain filters, e.g., LZW followed by ASCII85 encoding.
/// This benchmark simulates common pipelines.
fn bench_chained(c: &mut Criterion) {
    let mut group = c.benchmark_group("filters_chained");

    // Generate test data for chained pipelines
    let raw_10k = generate_raw_bytes(10 * 1024);

    // Pipeline 1: ASCII85 -> LZW (common in older PDFs)
    // Data is first LZW compressed, then ASCII85 encoded
    let lzw_compressed = lzw_encode(&raw_10k);
    let ascii85_lzw = ascii85_encode(&lzw_compressed);

    // Pipeline 2: RunLength -> LZW
    // Data is first LZW compressed, then RunLength encoded
    let rl_lzw = runlength_encode(&lzw_compressed);

    // Pipeline 3: ASCII85 -> RunLength
    // Data is first RunLength compressed, then ASCII85 encoded
    let rl_compressed = runlength_encode(&raw_10k);
    let ascii85_rl = ascii85_encode(&rl_compressed);

    // Benchmark: ASCII85 decode followed by LZW decode
    group.bench_function("ascii85_then_lzw_10K", |b| {
        b.iter(|| {
            let intermediate = ascii85decode(black_box(&ascii85_lzw)).unwrap();
            lzwdecode(black_box(&intermediate))
        })
    });

    // Benchmark: RunLength decode followed by LZW decode
    group.bench_function("runlength_then_lzw_10K", |b| {
        b.iter(|| {
            let intermediate = rldecode(black_box(&rl_lzw)).unwrap();
            lzwdecode(black_box(&intermediate))
        })
    });

    // Benchmark: ASCII85 decode followed by RunLength decode
    group.bench_function("ascii85_then_runlength_10K", |b| {
        b.iter(|| {
            let intermediate = ascii85decode(black_box(&ascii85_rl)).unwrap();
            rldecode(black_box(&intermediate))
        })
    });

    // Triple chain: ASCII85 -> LZW -> RunLength (extreme case)
    let lzw_compressed_small = lzw_encode(&generate_raw_bytes(1024));
    let rl_lzw_small = runlength_encode(&lzw_compressed_small);
    let ascii85_rl_lzw = ascii85_encode(&rl_lzw_small);

    group.bench_function("ascii85_runlength_lzw_1K", |b| {
        b.iter(|| {
            let step1 = ascii85decode(black_box(&ascii85_rl_lzw)).unwrap();
            let step2 = rldecode(black_box(&step1)).unwrap();
            lzwdecode(black_box(&step2))
        })
    });

    group.finish();
}

/// Benchmark ASCII85 decoding with random data (worst case).
fn bench_ascii85_random(c: &mut Criterion) {
    let mut group = c.benchmark_group("filters_ascii85_random");
    let data = TestData::new();

    for (name, encoded) in [
        ("1K", &data.ascii85_random_1k),
        ("10K", &data.ascii85_random_10k),
        ("100K", &data.ascii85_random_100k),
    ] {
        group.bench_with_input(BenchmarkId::from_parameter(name), encoded, |b, encoded| {
            b.iter(|| ascii85decode(black_box(encoded)))
        });
    }

    group.finish();
}

/// Benchmark LZW decoding with random data (worst case).
fn bench_lzw_random(c: &mut Criterion) {
    let mut group = c.benchmark_group("filters_lzw_random");
    let data = TestData::new();

    for (name, encoded) in [
        ("1K", &data.lzw_random_1k),
        ("10K", &data.lzw_random_10k),
        ("100K", &data.lzw_random_100k),
    ] {
        group.bench_with_input(BenchmarkId::from_parameter(name), encoded, |b, encoded| {
            b.iter(|| lzwdecode(black_box(encoded)))
        });
    }

    group.finish();
}

/// Benchmark RunLength decoding with random data (worst case).
fn bench_runlength_random(c: &mut Criterion) {
    let mut group = c.benchmark_group("filters_runlength_random");
    let data = TestData::new();

    for (name, encoded) in [
        ("1K", &data.runlength_random_1k),
        ("10K", &data.runlength_random_10k),
        ("100K", &data.runlength_random_100k),
    ] {
        group.bench_with_input(BenchmarkId::from_parameter(name), encoded, |b, encoded| {
            b.iter(|| rldecode(black_box(encoded)))
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_ascii85,
    bench_lzw,
    bench_runlength,
    bench_ascii85_random,
    bench_lzw_random,
    bench_runlength_random,
    bench_chained
);
criterion_main!(benches);
