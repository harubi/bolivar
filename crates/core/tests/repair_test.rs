use std::path::Path;

fn load_pdf(path: &str) -> Vec<u8> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let full = root.join(path);
    std::fs::read(&full).expect("fixture read")
}

fn first_mediabox(bytes: &[u8]) -> Option<(f64, f64, f64, f64)> {
    let needle = b"/MediaBox";
    let pos = bytes.windows(needle.len()).position(|w| w == needle)? + needle.len();
    let mut i = pos;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'[' {
        return None;
    }
    i += 1;
    let start = i;
    while i < bytes.len() && bytes[i] != b']' {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let content = &bytes[start..i];
    let mut nums = Vec::new();
    let mut j = 0;
    while j < content.len() {
        while j < content.len() && content[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= content.len() {
            break;
        }
        let k = j;
        while j < content.len() && !content[j].is_ascii_whitespace() {
            j += 1;
        }
        let token = std::str::from_utf8(&content[k..j]).ok()?;
        if let Ok(v) = token.parse::<f64>() {
            nums.push(v);
        }
    }
    if nums.len() >= 4 {
        Some((nums[0], nums[1], nums[2], nums[3]))
    } else {
        None
    }
}

#[test]
fn repair_malformed_pdf() {
    let input = load_pdf("references/pdfplumber/tests/pdfs/malformed-from-issue-932.pdf");
    let (_, y0, _, y1) = first_mediabox(&input).expect("mediabox");
    assert!(y0 > y1);

    let repaired = bolivar_core::document::repair::repair_bytes(&input).unwrap();
    let (_, ry0, _, ry1) = first_mediabox(&repaired).expect("mediabox");
    assert!(ry0 <= ry1);
}
