#[path = "common/criterion.rs"]
mod bench_criterion;
#[path = "common/seed.rs"]
mod bench_seed;
#[path = "common/tier.rs"]
mod bench_tier;
#[path = "common/bytes_throughput.rs"]
mod bytes_throughput;
#[path = "common/group_light.rs"]
mod group_light;

use std::hint::black_box;

use criterion::{BenchmarkId, criterion_group, criterion_main};

use bolivar_core::image::apply_png_predictor_with_mode;

use bench_criterion::{BenchCriterion, bench_criterion};
use bench_seed::bench_seed;
use bench_tier::{BenchTier, bench_tier};
use bytes_throughput::bytes_throughput;
use group_light::configure_group_light;

#[inline]
fn next_u64(state: &mut u64) -> u64 {
    let mut x = (*state).max(1);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

fn build_up_predictor_data(seed: u64, row_bytes: usize, rows: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(rows * (row_bytes + 1));
    let mut state = seed.max(1);
    for _ in 0..rows {
        data.extend(std::iter::once(2_u8));
        for _ in 0..row_bytes {
            data.push((next_u64(&mut state) & 0xff) as u8);
        }
    }
    data
}

fn bench_png_predictor_modes(c: &mut BenchCriterion) {
    let tier = bench_tier();
    let seed = bench_seed();
    let cases: &[(usize, usize)] = if tier == BenchTier::Quick {
        &[(96, 64), (512, 32)]
    } else {
        &[(96, 64), (512, 128), (2048, 64)]
    };

    let mut group = c.benchmark_group("png_predictor_modes");
    configure_group_light(&mut group, tier);

    for &(row_bytes, rows) in cases {
        let data = build_up_predictor_data(
            seed ^ ((row_bytes as u64) << 16) ^ rows as u64,
            row_bytes,
            rows,
        );
        let columns = row_bytes;
        let case_id = format!("{row_bytes}x{rows}");

        let scalar_out = apply_png_predictor_with_mode(&data, columns, 1, 8, false).unwrap();
        let simd_out = apply_png_predictor_with_mode(&data, columns, 1, 8, true).unwrap();
        assert_eq!(scalar_out, simd_out);

        group.throughput(bytes_throughput(data.len()));
        group.bench_with_input(BenchmarkId::new("scalar", &case_id), &data, |b, input| {
            b.iter(|| {
                let out =
                    apply_png_predictor_with_mode(black_box(input), columns, 1, 8, false).unwrap();
                black_box(out.len());
            })
        });
        group.bench_with_input(BenchmarkId::new("simd", &case_id), &data, |b, input| {
            b.iter(|| {
                let out =
                    apply_png_predictor_with_mode(black_box(input), columns, 1, 8, true).unwrap();
                black_box(out.len());
            })
        });
    }

    group.finish();
}

criterion_group!(
    name = png_predictor_benches;
    config = bench_criterion();
    targets = bench_png_predictor_modes
);
criterion_main!(png_predictor_benches);
