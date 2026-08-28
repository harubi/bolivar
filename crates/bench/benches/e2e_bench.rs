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
use bolivar_core::error::Result as CoreResult;
use bolivar_core::extract::{
    ExtractOptions, extract_pages_stream_from_doc, extract_tables_stream_from_doc_with_settings,
    extract_text,
};
use bolivar_core::layout::LAParams;
use bolivar_core::pdfpage::PDFPage;
use bolivar_core::table::TableSettings;

use bench_criterion::{BenchCriterion, bench_criterion};
use bench_tier::bench_tier;
use bytes_throughput::bytes_throughput;
use fixtures::load_fixtures;
use group_heavy::configure_group_heavy;
use group_light::configure_group_light;

fn bench_parse_only(c: &mut BenchCriterion) {
    let tier = bench_tier();
    let fixtures = load_fixtures(None);

    let mut group = c.benchmark_group("e2e_parse_only");
    configure_group_light(&mut group, tier);

    for fx in fixtures {
        group.throughput(bytes_throughput(fx.bytes.len()));
        group.bench_with_input(
            BenchmarkId::new("parse", &fx.meta.id),
            &fx.bytes,
            |b, data| {
                b.iter(|| {
                    let doc = PDFDocument::new(data, "").expect("parse PDF");
                    let mut count = 0usize;
                    for page in PDFPage::create_pages(&doc) {
                        page.expect("parse page");
                        count += 1;
                    }
                    black_box(count);
                })
            },
        );
    }

    group.finish();
}

fn bench_extract_text(c: &mut BenchCriterion) {
    let tier = bench_tier();
    let fixtures = load_fixtures(Some("text"));

    let mut group = c.benchmark_group("e2e_extract_text");
    configure_group_heavy(&mut group, tier);

    for fx in fixtures {
        let options = ExtractOptions {
            laparams: Some(LAParams::default()),
            ..Default::default()
        };
        group.throughput(bytes_throughput(fx.bytes.len()));
        group.bench_with_input(
            BenchmarkId::new("text", &fx.meta.id),
            &fx.bytes,
            |b, data| {
                b.iter(|| {
                    let text = extract_text(data, Some(options.clone())).expect("extract text");
                    black_box(text.len());
                })
            },
        );
    }

    group.finish();
}

fn bench_extract_pages_doc_reuse(c: &mut BenchCriterion) {
    let tier = bench_tier();
    let fixtures = load_fixtures(Some("layout"));

    let mut group = c.benchmark_group("e2e_extract_pages_doc_reuse");
    configure_group_heavy(&mut group, tier);

    for fx in fixtures {
        let doc = std::sync::Arc::new(PDFDocument::new(&fx.bytes, "").expect("parse PDF"));
        let options = ExtractOptions {
            laparams: Some(LAParams::default()),
            ..Default::default()
        };
        group.throughput(bytes_throughput(fx.bytes.len()));
        group.bench_with_input(BenchmarkId::new("pages", &fx.meta.id), &doc, |b, doc| {
            b.iter(|| {
                let pages: Vec<_> =
                    extract_pages_stream_from_doc(std::sync::Arc::clone(doc), options.clone())
                        .expect("extract pages")
                        .map(|r| r.map(|(_, p)| p))
                        .collect::<CoreResult<Vec<_>>>()
                        .expect("extract pages");
                black_box(pages.len());
            })
        });
    }

    group.finish();
}

fn bench_extract_tables_e2e(c: &mut BenchCriterion) {
    let tier = bench_tier();
    let fixtures = load_fixtures(Some("tables"));
    let settings = TableSettings::default();

    let mut group = c.benchmark_group("e2e_extract_tables_one_pass");
    configure_group_heavy(&mut group, tier);

    for fx in fixtures {
        let doc = std::sync::Arc::new(PDFDocument::new(&fx.bytes, "").expect("parse PDF"));
        let options = ExtractOptions {
            laparams: Some(LAParams::default()),
            ..Default::default()
        };
        group.throughput(bytes_throughput(fx.bytes.len()));
        group.bench_with_input(BenchmarkId::new("tables", &fx.meta.id), &doc, |b, doc| {
            b.iter(|| {
                let stream = extract_tables_stream_from_doc_with_settings(
                    std::sync::Arc::clone(doc),
                    options.clone(),
                    settings.clone(),
                )
                .expect("extract tables");
                let mut count = 0usize;
                for item in stream {
                    let (_, tables) = item.expect("extract table page");
                    count += tables.len();
                }
                black_box(count);
            })
        });
    }

    group.finish();
}

criterion_group!(
    name = e2e_benches;
    config = bench_criterion();
    targets = bench_parse_only, bench_extract_text, bench_extract_pages_doc_reuse, bench_extract_tables_e2e
);
criterion_main!(e2e_benches);
