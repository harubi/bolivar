#[path = "common/criterion.rs"]
mod bench_criterion;
#[path = "common/tier.rs"]
mod bench_tier;
#[path = "common/group_light.rs"]
mod group_light;

use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, criterion_group, criterion_main};

use bolivar_core::engine::select_pages;

use bench_criterion::{BenchCriterion, bench_criterion};
use bench_tier::{BenchTier, bench_tier};
use group_light::configure_group_light;

/// Page selection with a selection list as long as the document, the shape a
/// "read the rest" cursor produces.
fn bench_select_pages(c: &mut BenchCriterion) {
    let tier = bench_tier();
    let sizes: &[usize] = if tier == BenchTier::Quick {
        &[1_000, 10_000]
    } else {
        &[1_000, 10_000, 50_000]
    };

    let mut group = c.benchmark_group("plan_select_pages");
    configure_group_light(&mut group, tier);

    for &n in sizes {
        let every_page: Vec<usize> = (0..n).collect();
        group.bench_with_input(BenchmarkId::new("full_list", n), &every_page, |b, pages| {
            // The list is cloned outside the timed part: select_pages takes it by value.
            b.iter_batched(
                || pages.clone(),
                |pages| black_box(select_pages(n, Some(pages), 0).len()),
                BatchSize::LargeInput,
            )
        });
    }

    group.finish();
}

criterion_group!(
    name = plan_benches;
    config = bench_criterion();
    targets = bench_select_pages
);
criterion_main!(plan_benches);
