//! PDF repair utilities.
//!
//! Current repair focuses on normalizing MediaBox/CropBox ordering
//! without altering file length (preserves xref offsets).

use crate::error::Result;

pub fn repair_bytes(input: &[u8]) -> Result<Vec<u8>> {
    let mut out = input.to_vec();
    let mut changed = false;
    changed |= fix_box(&mut out, b"/MediaBox");
    changed |= fix_box(&mut out, b"/CropBox");
    if changed { Ok(out) } else { Ok(out) }
}

fn fix_box(data: &mut [u8], key: &[u8]) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i + key.len() <= data.len() {
        if &data[i..i + key.len()] == key {
            let mut j = i + key.len();
            while j < data.len() && data[j].is_ascii_whitespace() {
                j += 1;
            }
            if j >= data.len() || data[j] != b'[' {
                i = j;
                continue;
            }
            j += 1;
            let content_start = j;
            while j < data.len() && data[j] != b']' {
                j += 1;
            }
            if j >= data.len() {
                break;
            }
            let content_end = j;
            let content = &data[content_start..content_end];
            if let Some(new_content) = reorder_box_content(content) {
                if new_content.len() == content.len() && new_content != content {
                    data[content_start..content_end].copy_from_slice(&new_content);
                    changed = true;
                }
            }
            i = content_end + 1;
            continue;
        }
        i += 1;
    }
    changed
}

fn reorder_box_content(content: &[u8]) -> Option<Vec<u8>> {
    let (mut tokens, seps, trailing) = parse_tokens(content)?;
    if tokens.len() != 4 || seps.len() != 4 {
        return None;
    }

    let nums: Vec<f64> = tokens
        .iter()
        .map(|t| std::str::from_utf8(t).ok()?.parse::<f64>().ok())
        .collect::<Option<Vec<f64>>>()?;

    let (x0, y0, x1, y1) = (nums[0], nums[1], nums[2], nums[3]);

    if x0 > x1 {
        tokens.swap(0, 2);
    }
    if y0 > y1 {
        tokens.swap(1, 3);
    }

    let mut out = Vec::with_capacity(content.len());
    for idx in 0..tokens.len() {
        out.extend_from_slice(&seps[idx]);
        out.extend_from_slice(&tokens[idx]);
    }
    out.extend_from_slice(&trailing);
    Some(out)
}

fn parse_tokens(content: &[u8]) -> Option<(Vec<Vec<u8>>, Vec<Vec<u8>>, Vec<u8>)> {
    let mut tokens = Vec::new();
    let mut seps = Vec::new();
    let mut sep = Vec::new();
    let mut i = 0;
    while i < content.len() {
        if content[i].is_ascii_whitespace() {
            sep.push(content[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < content.len() && !content[i].is_ascii_whitespace() {
            i += 1;
        }
        seps.push(std::mem::take(&mut sep));
        tokens.push(content[start..i].to_vec());
    }
    Some((tokens, seps, sep))
}
