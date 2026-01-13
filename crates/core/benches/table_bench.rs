#[path = "common/criterion.rs"]
mod bench_criterion;
#[path = "common/tier.rs"]
mod bench_tier;
#[path = "common/bytes_throughput.rs"]
mod bytes_throughput;
#[path = "common/fixtures.rs"]
mod fixtures;
#[path = "common/group_heavy.rs"]
mod group_heavy;
#[path = "common/group_light.rs"]
mod group_light;

use std::hint::black_box;

use criterion::{BenchmarkId, criterion_group, criterion_main};

use bolivar_core::document::PDFDocument;
use bolivar_core::high_level::{ExtractOptions, extract_pages_with_document};
use bolivar_core::layout::LAParams;
use bolivar_core::table::{
    PageGeometry, TableSettings, TextSettings, extract_tables_from_ltpage,
    extract_text_from_ltpage, extract_words_from_ltpage,
};

use bench_criterion::{BenchCriterion, bench_criterion};
use bench_tier::bench_tier;
use bytes_throughput::bytes_throughput;
use fixtures::load_fixtures;
use group_heavy::configure_group_heavy;
use group_light::configure_group_light;

fn bench_table_extract(c: &mut BenchCriterion) {
    let tier = bench_tier();
    let fixtures = load_fixtures(Some("tables"));
    let settings = TableSettings::default();

    let mut group = c.benchmark_group("table_extract_tables");
    configure_group_heavy(&mut group, tier);

    for fx in fixtures {
        let doc = PDFDocument::new(&fx.bytes, "").expect("parse PDF");
        let pages = extract_pages_with_document(
            &doc,
            ExtractOptions {
                laparams: Some(LAParams::default()),
                ..Default::default()
            },
        )
        .expect("extract pages");
        let geoms: Vec<PageGeometry> = pages
            .iter()
            .map(|page| {
                let bbox = page.bbox();
                PageGeometry {
                    page_bbox: bbox,
                    mediabox: bbox,
                    initial_doctop: 0.0,
                    force_crop: false,
                }
            })
            .collect();

        group.throughput(bytes_throughput(fx.bytes.len()));
        group.bench_with_input(
            BenchmarkId::new("tables", &fx.meta.id),
            &pages,
            |b, pages| {
                b.iter(|| {
                    let mut count = 0usize;
                    for (page, geom) in pages.iter().zip(geoms.iter()) {
                        let tables = extract_tables_from_ltpage(page, geom, &settings);
                        count += tables.len();
                    }
                    black_box(count);
                })
            },
        );
    }

    group.finish();
}

fn bench_text_extract(c: &mut BenchCriterion) {
    let tier = bench_tier();
    let fixtures = load_fixtures(Some("text"));
    let text_settings = TextSettings::default();

    let mut group = c.benchmark_group("table_extract_text");
    configure_group_light(&mut group, tier);

    for fx in fixtures {
        let doc = PDFDocument::new(&fx.bytes, "").expect("parse PDF");
        let pages = extract_pages_with_document(
            &doc,
            ExtractOptions {
                laparams: Some(LAParams::default()),
                ..Default::default()
            },
        )
        .expect("extract pages");
        let geoms: Vec<PageGeometry> = pages
            .iter()
            .map(|page| {
                let bbox = page.bbox();
                PageGeometry {
                    page_bbox: bbox,
                    mediabox: bbox,
                    initial_doctop: 0.0,
                    force_crop: false,
                }
            })
            .collect();

        group.throughput(bytes_throughput(fx.bytes.len()));
        group.bench_with_input(
            BenchmarkId::new("words", &fx.meta.id),
            &pages,
            |b, pages| {
                b.iter(|| {
                    let mut count = 0usize;
                    for (page, geom) in pages.iter().zip(geoms.iter()) {
                        let words = extract_words_from_ltpage(page, geom, text_settings.clone());
                        count += words.len();
                    }
                    black_box(count);
                })
            },
        );

        group.bench_with_input(BenchmarkId::new("text", &fx.meta.id), &pages, |b, pages| {
            b.iter(|| {
                let mut total = 0usize;
                for (page, geom) in pages.iter().zip(geoms.iter()) {
                    let text = extract_text_from_ltpage(page, geom, text_settings.clone());
                    total += text.len();
                }
                black_box(total);
            })
        });
    }

    group.finish();
}

criterion_group!(
    name = table_benches;
    config = bench_criterion();
    targets = bench_table_extract, bench_text_extract
);
criterion_main!(table_benches);
