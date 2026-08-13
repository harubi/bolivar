#[path = "common/criterion.rs"]
mod bench_criterion;
#[path = "common/seed.rs"]
mod bench_seed;
#[path = "common/tier.rs"]
mod bench_tier;
#[path = "common/group_heavy.rs"]
mod group_heavy;
#[path = "common/pages_throughput.rs"]
mod pages_throughput;
#[path = "common/rng.rs"]
mod rng;

use std::hint::black_box;

use criterion::{BenchmarkId, criterion_group, criterion_main};

use bolivar_core::layout::{
    LAParams, LTLayoutContainer, LTTextBoxHorizontal, LTTextLineHorizontal, TextBoxType,
    reconstruct_text_for_output,
};
use bolivar_core::utils::Rect;

use bench_criterion::{BenchCriterion, bench_criterion};
use bench_seed::bench_seed;
use bench_tier::{BenchTier, bench_tier};
use group_heavy::configure_group_heavy;
use pages_throughput::pages_throughput;
use rng::XorShift64;

const PAGE_BBOX: Rect = (0.0, 0.0, 612.0, 792.0);

fn generate_text_boxes(
    seed: u64,
    count: usize,
) -> (
    LTLayoutContainer,
    LAParams,
    Vec<bolivar_core::layout::TextBoxType>,
) {
    let laparams = LAParams::default();
    let container = LTLayoutContainer::new(PAGE_BBOX);
    let mut rng = XorShift64::new(seed);
    let mut boxes = Vec::with_capacity(count);
    for i in 0..count {
        let column = (i % 4) as f64;
        let row = (i / 4) as f64;
        let x0 = 24.0 + column * 145.0 + rng.gen_f64(0.0, 3.0);
        let y0 = 760.0 - row * 9.0 - rng.gen_f64(0.0, 2.0);
        let width = 48.0 + rng.gen_f64(0.0, 60.0);
        let height = 7.0 + rng.gen_f64(0.0, 3.0);

        let mut line = LTTextLineHorizontal::new(0.1);
        line.set_bbox((x0, y0, x0 + width, y0 + height));
        let mut text_box = LTTextBoxHorizontal::new();
        text_box.add(line);
        boxes.push(TextBoxType::Horizontal(text_box));
    }
    (container, laparams, boxes)
}

fn bench_group_textboxes_exact(c: &mut BenchCriterion) {
    let tier = bench_tier();
    let seed = bench_seed();
    let sizes: &[usize] = if tier == BenchTier::Quick {
        &[128, 338]
    } else {
        &[128, 338, 512, 768]
    };

    let mut group = c.benchmark_group("layout_group_textboxes_exact");
    configure_group_heavy(&mut group, tier);

    for &n in sizes {
        let (container, laparams, boxes) = generate_text_boxes(seed ^ (n as u64), n);
        group.throughput(pages_throughput(n));
        group.bench_with_input(BenchmarkId::new("exact", n), &boxes, |b, boxes| {
            b.iter(|| {
                let groups = container.group_textboxes_exact(&laparams, boxes);
                black_box(groups.len());
            })
        });
    }

    group.finish();
}

fn bench_bidi_reconstruction(c: &mut BenchCriterion) {
    let mut group = c.benchmark_group("bidi_reconstruction");
    for (name, text) in [
        ("ltr_fast_path", "Reference code 123456"),
        (
            "arabic",
            "123456 :\u{fe94}\u{fef4}\u{fe91}\u{feae}\u{fecc}\u{fedf}\u{fe8d}",
        ),
        (
            "mixed",
            "Task Ref42 12:34:\u{fe94}\u{fec8}\u{fea3}\u{fefc}\u{fee3}**56:78:\u{fe96}\u{fed7}\u{feee}\u{fedf}\u{fe8d}",
        ),
    ] {
        group.bench_function(name, |b| {
            b.iter(|| reconstruct_text_for_output(black_box(text)))
        });
    }
    group.finish();
}

criterion_group!(
    name = layout_benches;
    config = bench_criterion();
    targets = bench_group_textboxes_exact, bench_bidi_reconstruction
);
criterion_main!(layout_benches);
