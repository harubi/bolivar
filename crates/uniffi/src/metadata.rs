use std::collections::{BTreeMap, HashSet};

use bolivar_core::model::PDFObject;
use bolivar_core::pdfdocument::PDFDocument;
use bolivar_core::utils::decode_text;

use crate::types::{MetadataEntry, PdfPermissions, PdfVersion, RawDocumentMetadata, usize_to_u32};

fn object_text(document: &PDFDocument, object: &PDFObject, depth: usize) -> Option<String> {
    if depth == 0 {
        return None;
    }
    let resolved = document.resolve(object).ok()?;
    match resolved {
        PDFObject::Null => Some("null".to_owned()),
        PDFObject::Bool(value) => Some(value.to_string()),
        PDFObject::Int(value) => Some(value.to_string()),
        PDFObject::Real(value) => Some(value.to_string()),
        PDFObject::Name(value) => Some(value.to_string()),
        PDFObject::String(value) => Some(decode_text(&value)),
        PDFObject::Array(values) => {
            let values = values
                .iter()
                .filter_map(|value| object_text(document, value, depth - 1))
                .collect::<Vec<_>>();
            Some(format!("[{}]", values.join(", ")))
        }
        PDFObject::Dict(_) | PDFObject::Stream(_) | PDFObject::Ref(_) => None,
    }
}

fn document_info(document: &PDFDocument) -> BTreeMap<String, String> {
    let mut entries = BTreeMap::new();
    for dictionary in document.info() {
        for (key, value) in dictionary {
            if let Some(value) = object_text(document, value, 8) {
                entries.insert(key.to_string(), value);
            }
        }
    }
    entries
}

fn header_version(bytes: &[u8]) -> Option<String> {
    let header = bytes.get(..bytes.len().min(16))?;
    let marker = b"%PDF-";
    let start = header
        .windows(marker.len())
        .position(|part| part == marker)?
        + marker.len();
    let version = header[start..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit() || **byte == b'.')
        .copied()
        .collect::<Vec<_>>();
    (!version.is_empty()).then(|| String::from_utf8_lossy(&version).into_owned())
}

fn catalog_version(document: &PDFDocument) -> Option<String> {
    let value = document.catalog().get("Version")?;
    match document.resolve(value).ok()? {
        PDFObject::Name(version) => Some(version.to_string()),
        _ => None,
    }
}

fn catalog_dictionary(
    document: &PDFDocument,
    key: &str,
) -> Option<bolivar_core::pdftypes::PDFDict> {
    match document.resolve(document.catalog().get(key)?).ok()? {
        PDFObject::Dict(dictionary) => Some(dictionary),
        PDFObject::Stream(stream) => Some(stream.attrs),
        _ => None,
    }
}

fn dictionary_bool(dictionary: Option<&bolivar_core::pdftypes::PDFDict>, key: &str) -> bool {
    dictionary
        .and_then(|value| value.get(key))
        .and_then(|value| value.as_bool().ok())
        .unwrap_or(false)
}

fn contains_name(
    document: &PDFDocument,
    object: &PDFObject,
    expected: &str,
    depth: usize,
    visited: &mut HashSet<(u32, u32)>,
) -> bool {
    if depth == 0 {
        return false;
    }
    if let PDFObject::Ref(reference) = object
        && !visited.insert((reference.objid, reference.genno))
    {
        return false;
    }
    let Ok(resolved) = document.resolve(object) else {
        return false;
    };
    match resolved {
        PDFObject::Name(name) => name == expected,
        PDFObject::Array(values) => values
            .iter()
            .any(|value| contains_name(document, value, expected, depth - 1, visited)),
        PDFObject::Dict(dictionary) => dictionary.iter().any(|(key, value)| {
            key == expected || contains_name(document, value, expected, depth - 1, visited)
        }),
        PDFObject::Stream(stream) => stream.attrs.iter().any(|(key, value)| {
            key == expected || contains_name(document, value, expected, depth - 1, visited)
        }),
        _ => false,
    }
}

fn has_javascript(document: &PDFDocument) -> bool {
    ["Names", "OpenAction", "AA"].iter().any(|key| {
        document.catalog().get(*key).is_some_and(|object| {
            contains_name(document, object, "JavaScript", 12, &mut HashSet::new())
        })
    })
}

fn xmp_metadata(document: &PDFDocument) -> Option<String> {
    let metadata = document.catalog().get("Metadata")?;
    match document.resolve(metadata).ok()? {
        PDFObject::Stream(stream) => document
            .decode_stream(&stream)
            .ok()
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned()),
        _ => None,
    }
}

fn form_type(document: &PDFDocument) -> String {
    let Some(form) = catalog_dictionary(document, "AcroForm") else {
        return "none".to_owned();
    };
    if form.contains_key("XFA") {
        "xfa".to_owned()
    } else {
        "acroform".to_owned()
    }
}

fn is_linearized(bytes: &[u8]) -> bool {
    bytes.get(..bytes.len().min(2048)).is_some_and(|prefix| {
        prefix
            .windows(b"/Linearized".len())
            .any(|part| part == b"/Linearized")
    })
}

fn take_component(value: &str, start: usize, length: usize) -> Option<&str> {
    let component = value.get(start..start + length)?;
    component
        .bytes()
        .all(|byte| byte.is_ascii_digit())
        .then_some(component)
}

fn pdf_date_to_iso(raw: &str) -> Option<String> {
    let value = raw.strip_prefix("D:").unwrap_or(raw);
    let year = take_component(value, 0, 4)?;
    let mut iso = year.to_owned();
    let components = [(4, "-"), (6, "-"), (8, "T"), (10, ":"), (12, ":")];
    let mut consumed = 4;
    for (start, separator) in components {
        let Some(component) = take_component(value, start, 2) else {
            break;
        };
        iso.push_str(separator);
        iso.push_str(component);
        consumed = start + 2;
    }

    let timezone = value.get(consumed..).unwrap_or_default();
    if timezone.starts_with('Z') {
        iso.push('Z');
    } else if matches!(timezone.as_bytes().first(), Some(b'+') | Some(b'-')) {
        let sign = timezone.as_bytes()[0] as char;
        let digits = timezone[1..]
            .chars()
            .filter(char::is_ascii_digit)
            .collect::<String>();
        if digits.len() >= 2 {
            iso.push(sign);
            iso.push_str(&digits[..2]);
            iso.push(':');
            iso.push_str(digits.get(2..4).unwrap_or("00"));
        }
    }
    Some(iso)
}

pub(crate) fn metadata_from_document(document: &PDFDocument) -> RawDocumentMetadata {
    let info = document_info(document);
    let title = info.get("Title").cloned();
    let author = info.get("Author").cloned();
    let subject = info.get("Subject").cloned();
    let keywords = info.get("Keywords").cloned();
    let creator = info.get("Creator").cloned();
    let producer = info.get("Producer").cloned();
    let header = header_version(document.bytes());
    let catalog = catalog_version(document);
    let effective = catalog.clone().or_else(|| header.clone());
    let mark_info = catalog_dictionary(document, "MarkInfo");
    let creation_date_raw = info.get("CreationDate").cloned();
    let modification_date_raw = info.get("ModDate").cloned();
    let xmp_metadata = xmp_metadata(document);

    RawDocumentMetadata {
        document_info: info
            .into_iter()
            .map(|(key, value)| MetadataEntry { key, value })
            .collect(),
        title,
        author,
        subject,
        keywords,
        creator,
        producer,
        creation_date_iso: creation_date_raw.as_deref().and_then(pdf_date_to_iso),
        creation_date_raw,
        modification_date_iso: modification_date_raw.as_deref().and_then(pdf_date_to_iso),
        modification_date_raw,
        version: PdfVersion {
            header,
            catalog,
            effective,
        },
        file_size_bytes: document.bytes().len() as u64,
        page_count: usize_to_u32(document.page_count()),
        encrypted: document.is_encrypted(),
        permissions: PdfPermissions {
            printable: document.is_printable(),
            modifiable: document.is_modifiable(),
            extractable: document.is_extractable(),
        },
        linearized: is_linearized(document.bytes()),
        tagged: document.catalog().contains_key("StructTreeRoot")
            || dictionary_bool(mark_info.as_ref(), "Marked"),
        user_properties: dictionary_bool(mark_info.as_ref(), "UserProperties"),
        suspects: dictionary_bool(mark_info.as_ref(), "Suspects"),
        form: form_type(document),
        has_javascript: has_javascript(document),
        has_metadata_stream: xmp_metadata.is_some(),
        xmp_metadata,
    }
}

#[cfg(test)]
mod tests {
    use super::pdf_date_to_iso;

    #[test]
    fn pdf_date_preserves_partial_values_and_normalizes_offsets() {
        assert_eq!(pdf_date_to_iso("D:2026"), Some("2026".to_owned()));
        assert_eq!(
            pdf_date_to_iso("D:20260704114317+03'00'"),
            Some("2026-07-04T11:43:17+03:00".to_owned())
        );
    }
}
