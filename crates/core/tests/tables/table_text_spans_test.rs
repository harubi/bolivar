//! Every text span must address the cell text it claims to describe.
//!
//! Bidi reconstruction reorders words, so a consumer that wants to know which
//! page word produced a character has only the span list to go on. If a span's
//! offsets do not slice its own text back out, the list is worse than useless:
//! it points a parser at the wrong characters.
//!
//! This drives the same stream the `pdf2txt` CLI uses, because that is the path
//! the corpus is extracted with. The fixture is a directory of real PDFs named
//! by `BOLIVAR_SPANS_PDF_DIR` - deliberately not committed, these are customer
//! statements - and the test skips when the variable is unset.

use bolivar_core::document::PDFDocument;
use bolivar_core::engine::ExtractOptions;
use bolivar_core::extract::extract_tables_with_text_spans_stream_from_doc_with_settings;
use bolivar_core::table::TableSettings;
use std::sync::Arc;

fn pdf_paths() -> Vec<std::path::PathBuf> {
    let Ok(dir) = std::env::var("BOLIVAR_SPANS_PDF_DIR") else {
        return Vec::new();
    };
    let limit: usize = std::env::var("BOLIVAR_SPANS_PDF_LIMIT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut paths: Vec<std::path::PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "pdf"))
        .collect();
    paths.sort();
    paths.truncate(limit);
    paths
}

/// The settings the corpus is extracted with.
fn corpus_settings() -> TableSettings {
    let mut settings = TableSettings {
        snap_x_tolerance: 8.0,
        snap_y_tolerance: 7.0,
        intersection_x_tolerance: 59.0,
        intersection_y_tolerance: 5.0,
        join_x_tolerance: 20.0,
        join_y_tolerance: 20.0,
        ..Default::default()
    };
    settings.text_settings.bidi = true;
    settings
}

#[test]
fn every_span_slices_its_own_text_out_of_the_cell() {
    let paths = pdf_paths();
    if paths.is_empty() {
        eprintln!("BOLIVAR_SPANS_PDF_DIR unset or empty; skipping");
        return;
    }

    let mut documents = 0usize;
    let mut cells = 0usize;
    let mut spans_checked = 0usize;
    let mut bad_offsets = 0usize;
    let mut reordered = 0usize;

    for path in &paths {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let Ok(document) = PDFDocument::new(&bytes, "") else {
            continue;
        };
        documents += 1;

        let options = ExtractOptions {
            bidi: true,
            ..Default::default()
        };

        let Ok(stream) = extract_tables_with_text_spans_stream_from_doc_with_settings(
            Arc::new(document),
            options,
            corpus_settings(),
        ) else {
            continue;
        };

        for page in stream {
            let Ok((_page_number, (tables, spans))) = page else {
                continue;
            };
            for (table_index, table) in tables.iter().enumerate() {
                let Some(table_spans) = spans.get(table_index) else {
                    continue;
                };
                for (row, span_row) in table.iter().zip(table_spans.iter()) {
                    for (cell, cell_spans) in row.iter().zip(span_row.iter()) {
                        let (Some(text), Some(cell_spans)) = (cell, cell_spans) else {
                            continue;
                        };
                        if cell_spans.is_empty() {
                            continue;
                        }
                        cells += 1;
                        let characters: Vec<char> = text.chars().collect();
                        let mut previous: Option<usize> = None;
                        for span in cell_spans {
                            spans_checked += 1;
                            let sliced: String = characters
                                .get(span.start..span.end)
                                .map(|slice| slice.iter().collect())
                                .unwrap_or_default();
                            if sliced != span.text {
                                bad_offsets += 1;
                            }
                            if previous.is_some_and(|earlier| span.word_index < earlier) {
                                reordered += 1;
                            }
                            previous = Some(span.word_index);
                        }
                    }
                }
            }
        }
    }

    eprintln!(
        "documents {documents}, cells {cells}, spans {spans_checked}, \
         reordered {reordered}, bad offsets {bad_offsets}"
    );
    assert!(spans_checked > 0, "no spans were produced to check");
    assert_eq!(
        bad_offsets, 0,
        "{bad_offsets} spans did not slice their own text"
    );
}
