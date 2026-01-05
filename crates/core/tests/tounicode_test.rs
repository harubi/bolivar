use bolivar_core::cmapdb::parse_tounicode_cmap;
use bolivar_core::pdfdocument::PDFDocument;
use bolivar_core::pdfpage::PDFPage;
use bolivar_core::pdftypes::PDFObject;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("references")
        .join("pdfplumber")
        .join("tests")
        .join("pdfs")
}

fn resolve_dict(
    doc: &PDFDocument,
    obj: &PDFObject,
) -> Option<std::collections::HashMap<String, PDFObject>> {
    doc.resolve(obj)
        .ok()
        .and_then(|o| o.as_dict().ok().cloned())
}

#[test]
fn test_tounicode_maps_simple_font_cid_67() {
    let pdf_path = fixtures_dir().join("pr-138-example.pdf");
    let data = std::fs::read(pdf_path).expect("missing pr-138-example.pdf fixture");
    let doc = PDFDocument::new(&data, "").expect("failed to parse PDF");

    let page = PDFPage::create_pages(&doc)
        .next()
        .expect("no pages")
        .expect("page parse error");

    let fonts_obj = page.resources.get("Font").expect("no Font resources");
    let fonts = resolve_dict(&doc, fonts_obj).expect("failed to resolve Font dict");

    // Collect all HelveticaNeueLTPro-Lt ToUnicode candidates (order is non-deterministic).
    let mut candidates: Vec<(String, Vec<u8>)> = Vec::new();
    for (_name, spec_obj) in fonts.iter() {
        let spec = resolve_dict(&doc, spec_obj).expect("failed to resolve font spec");
        let basefont = spec
            .get("BaseFont")
            .and_then(|v| v.as_name().ok())
            .unwrap_or("");
        if basefont.contains("HelveticaNeueLTPro-Lt") {
            let tounicode = match spec.get("ToUnicode") {
                Some(value) => value,
                None => continue,
            };
            let stream = match doc.resolve(tounicode).expect("ToUnicode resolve failed") {
                PDFObject::Stream(s) => s,
                _ => continue,
            };
            let decoded = doc
                .decode_stream(&stream)
                .expect("failed to decode ToUnicode stream");
            candidates.push((basefont.to_string(), decoded));
        }
    }

    assert!(
        !candidates.is_empty(),
        "HelveticaNeueLTPro-Lt font not found"
    );

    let debug = std::env::var("BOLIVAR_DEBUG_TOUNICODE").is_ok();
    for (basefont, data) in candidates.into_iter() {
        if debug {
            let nul_count = data.iter().filter(|&&b| b == 0).count();
            println!(
                "ToUnicode font={}, bytes len={}, nul_count={}",
                basefont,
                data.len(),
                nul_count
            );
            let preview: Vec<u8> = data.iter().cloned().take(80).collect();
            println!("ToUnicode preview: {:?}", preview);
        }
        let content = String::from_utf8_lossy(&data);
        if !content.contains("beginbfchar") && !content.contains("beginbfrange") {
            continue;
        }
        let cmap = parse_tounicode_cmap(&data);
        let space = match cmap.get_unichr(0x20) {
            Some(value) => value,
            None => continue,
        };
        if space != " " {
            continue;
        }
        let mapped = match cmap.get_unichr(0x43) {
            Some(value) => value,
            None => continue,
        };
        assert_eq!(mapped, "C");
        return;
    }

    panic!("No matching ToUnicode map contained CID 0x20 and 0x43 for HelveticaNeueLTPro-Lt");
}
