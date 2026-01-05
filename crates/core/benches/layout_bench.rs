mod common;

use std::hint::black_box;

use criterion::{BenchmarkId, criterion_group, criterion_main};

use bolivar_core::layout::{LAParams, LTChar, LTLayoutContainer};
use bolivar_core::utils::Rect;

use common::{
    BenchCriterion, GroupWeight, XorShift64, bench_config, bench_criterion, configure_group,
    pages_throughput,
};

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

    let mut chars = Vec::with_capacity(count * 12);
    for i in 0..count {
        let col = (i % 3) as f64;
        let row = (i / 3) as f64;
        let base_x = 36.0 + col * 180.0 + rng.gen_f64(0.0, 4.0);
        let base_y = 720.0 - row * 24.0 - rng.gen_f64(0.0, 4.0);
        let line_h = 9.0 + (i % 3) as f64;
        let char_w = 5.0 + (i % 4) as f64;

        for line in 0..2 {
            let y0 = base_y - (line as f64 + 1.0) * (line_h + 2.0);
            let y1 = y0 + line_h;
            for ch in 0..8 {
                let x0 = base_x + (ch as f64) * char_w;
                let x1 = x0 + char_w;
                chars.push(LTChar::new(
                    (x0, y0, x1, y1),
                    "a",
                    "Helvetica",
                    line_h,
                    true,
                    char_w,
                ));
            }
        }
    }

    let lines = container.group_objects(&laparams, &chars);
    let boxes = container.group_textlines(&laparams, lines);
    (container, laparams, boxes)
}

fn bench_group_textboxes_exact(c: &mut BenchCriterion) {
    let cfg = bench_config();
    let sizes: &[usize] = if cfg.tier == common::BenchTier::Quick {
        &[120, 240]
    } else {
        &[120, 240, 480]
    };

    let mut group = c.benchmark_group("layout_group_textboxes_exact");
    configure_group(&mut group, &cfg, GroupWeight::Heavy);

    for &n in sizes {
        let (container, laparams, boxes) = generate_text_boxes(cfg.seed ^ (n as u64), n);
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

criterion_group!(
    name = layout_benches;
    config = bench_criterion();
    targets = bench_group_textboxes_exact
);
criterion_main!(layout_benches);
