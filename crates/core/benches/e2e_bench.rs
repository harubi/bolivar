mod common;

use std::hint::black_box;

use criterion::{BenchmarkId, criterion_group, criterion_main};

use bolivar_core::document::PDFDocument;
use bolivar_core::high_level::{ExtractOptions, extract_pages_with_document, extract_text};
use bolivar_core::layout::LAParams;
use bolivar_core::pdfpage::PDFPage;
use bolivar_core::table::{PageGeometry, TableSettings, extract_tables_from_ltpage};

use common::{
    BenchCriterion, GroupWeight, bench_config, bench_criterion, bytes_throughput, configure_group,
    load_fixtures, pages_throughput,
};

fn bench_parse_only(c: &mut BenchCriterion) {
    let cfg = bench_config();
    let fixtures = load_fixtures(None);

    let mut group = c.benchmark_group("e2e_parse_only");
    configure_group(&mut group, &cfg, GroupWeight::Light);

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
    let cfg = bench_config();
    let fixtures = load_fixtures(Some("text"));

    let mut group = c.benchmark_group("e2e_extract_text");
    configure_group(&mut group, &cfg, GroupWeight::Heavy);

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
    let cfg = bench_config();
    let fixtures = load_fixtures(Some("layout"));

    let mut group = c.benchmark_group("e2e_extract_pages_doc_reuse");
    configure_group(&mut group, &cfg, GroupWeight::Heavy);

    for fx in fixtures {
        let doc = PDFDocument::new(&fx.bytes, "").expect("parse PDF");
        let options = ExtractOptions {
            laparams: Some(LAParams::default()),
            ..Default::default()
        };
        group.throughput(bytes_throughput(fx.bytes.len()));
        group.bench_with_input(BenchmarkId::new("pages", &fx.meta.id), &doc, |b, doc| {
            b.iter(|| {
                let pages =
                    extract_pages_with_document(doc, options.clone()).expect("extract pages");
                black_box(pages.len());
            })
        });
    }

    group.finish();
}

fn bench_extract_tables_e2e(c: &mut BenchCriterion) {
    let cfg = bench_config();
    let fixtures = load_fixtures(Some("tables"));
    let settings = TableSettings::default();

    let mut group = c.benchmark_group("e2e_extract_tables");
    configure_group(&mut group, &cfg, GroupWeight::Heavy);

    for fx in fixtures {
        let doc = PDFDocument::new(&fx.bytes, "").expect("parse PDF");
        let options = ExtractOptions {
            laparams: Some(LAParams::default()),
            ..Default::default()
        };
        let pages = extract_pages_with_document(&doc, options.clone()).expect("extract pages");
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

        group.throughput(pages_throughput(pages.len()));
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

criterion_group!(
    name = e2e_benches;
    config = bench_criterion();
    targets = bench_parse_only, bench_extract_text, bench_extract_pages_doc_reuse, bench_extract_tables_e2e
);
criterion_main!(e2e_benches);
