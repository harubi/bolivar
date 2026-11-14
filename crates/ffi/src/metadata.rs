use std::collections::{BTreeMap, HashSet};

use bolivar_core::model::PDFObject;
use bolivar_core::pdfdocument::PDFDocument;
use bolivar_core::utils::decode_text;

use crate::abi::FfiResult;
use crate::batch::{Builder, Built, metadata as schema};

/// Document metadata, before it is written into a batch.
#[derive(Debug)]
pub struct Metadata {
    pub document_info: Vec<(String, String)>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub creation_date_raw: Option<String>,
    pub creation_date_iso: Option<String>,
    pub modification_date_raw: Option<String>,
    pub modification_date_iso: Option<String>,
    pub version_header: Option<String>,
    pub version_catalog: Option<String>,
    pub file_size_bytes: u64,
    pub page_count: usize,
    pub encrypted: bool,
    pub printable: bool,
    pub modifiable: bool,
    pub extractable: bool,
    pub linearized: bool,
    pub tagged: bool,
    pub user_properties: bool,
    pub suspects: bool,
    pub form: String,
    pub has_javascript: bool,
    pub xmp_metadata: Option<String>,
}

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
    let dictionary = match resolved {
        PDFObject::Name(name) => return name == expected,
        PDFObject::Array(values) => {
            return values
                .iter()
                .any(|value| contains_name(document, value, expected, depth - 1, visited));
        }
        PDFObject::Dict(dictionary) => dictionary,
        PDFObject::Stream(stream) => stream.attrs,
        _ => return false,
    };
    dictionary.iter().any(|(key, value)| {
        key == expected || contains_name(document, value, expected, depth - 1, visited)
    })
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

pub fn metadata_from_document(document: &PDFDocument) -> Metadata {
    let info = document_info(document);
    let header = header_version(document.bytes());
    let catalog = catalog_version(document);
    let mark_info = catalog_dictionary(document, "MarkInfo");
    let creation_date_raw = info.get("CreationDate").cloned();
    let modification_date_raw = info.get("ModDate").cloned();
    let xmp_metadata = xmp_metadata(document);

    Metadata {
        title: info.get("Title").cloned(),
        author: info.get("Author").cloned(),
        subject: info.get("Subject").cloned(),
        keywords: info.get("Keywords").cloned(),
        creator: info.get("Creator").cloned(),
        producer: info.get("Producer").cloned(),
        creation_date_iso: creation_date_raw.as_deref().and_then(pdf_date_to_iso),
        creation_date_raw,
        modification_date_iso: modification_date_raw.as_deref().and_then(pdf_date_to_iso),
        modification_date_raw,
        version_header: header,
        version_catalog: catalog,
        file_size_bytes: document.bytes().len() as u64,
        page_count: document.page_count(),
        encrypted: document.is_encrypted(),
        printable: document.is_printable(),
        modifiable: document.is_modifiable(),
        extractable: document.is_extractable(),
        linearized: is_linearized(document.bytes()),
        tagged: document.catalog().contains_key("StructTreeRoot")
            || dictionary_bool(mark_info.as_ref(), "Marked"),
        user_properties: dictionary_bool(mark_info.as_ref(), "UserProperties"),
        suspects: dictionary_bool(mark_info.as_ref(), "Suspects"),
        form: form_type(document),
        has_javascript: has_javascript(document),
        xmp_metadata,
        // Sorted by key, one entry per key, as the info dictionaries merge.
        document_info: info.into_iter().collect(),
    }
}

/// Write metadata into a batch: named ranges and flag bits in schema order.
pub fn metadata_built(m: &Metadata) -> FfiResult<Built> {
    use schema::{Col, Flag, Int, Range};
    let mut b = Builder::new(&schema::SCHEMA);
    let ranges: [(Range, Option<&str>); 14] = [
        (Range::Title, m.title.as_deref()),
        (Range::Author, m.author.as_deref()),
        (Range::Subject, m.subject.as_deref()),
        (Range::Keywords, m.keywords.as_deref()),
        (Range::Creator, m.creator.as_deref()),
        (Range::Producer, m.producer.as_deref()),
        (Range::CreationDateRaw, m.creation_date_raw.as_deref()),
        (Range::CreationDateIso, m.creation_date_iso.as_deref()),
        (
            Range::ModificationDateRaw,
            m.modification_date_raw.as_deref(),
        ),
        (
            Range::ModificationDateIso,
            m.modification_date_iso.as_deref(),
        ),
        (Range::VersionHeader, m.version_header.as_deref()),
        (Range::VersionCatalog, m.version_catalog.as_deref()),
        (Range::Form, Some(m.form.as_str())),
        (Range::Xmp, m.xmp_metadata.as_deref()),
    ];
    for (index, (slot, value)) in ranges.into_iter().enumerate() {
        debug_assert_eq!(slot as usize, index, "ranges are written in schema order");
        let range = b.text_opt(value)?;
        b.i32s(Col::Ranges).extend(range);
    }
    for (key, value) in &m.document_info {
        let key = b.text(key)?;
        b.i32s(Col::InfoKeys).extend(key);
        let value = b.text(value)?;
        b.i32s(Col::InfoValues).extend(value);
    }
    let flags = [
        (Flag::Encrypted, m.encrypted),
        (Flag::Printable, m.printable),
        (Flag::Modifiable, m.modifiable),
        (Flag::Extractable, m.extractable),
        (Flag::Linearized, m.linearized),
        (Flag::Tagged, m.tagged),
        (Flag::UserProperties, m.user_properties),
        (Flag::Suspects, m.suspects),
        (Flag::HasJavascript, m.has_javascript),
    ]
    .into_iter()
    .filter(|(_, set)| *set)
    .fold(0_i64, |bits, (flag, _)| bits | (1 << flag as u32));
    b.int(
        Int::FileSize,
        i64::try_from(m.file_size_bytes).unwrap_or(i64::MAX),
    );
    b.int(Int::PageCount, m.page_count as i64);
    b.int(Int::Flags, flags);
    b.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::tests::View;

    fn metadata() -> Metadata {
        Metadata {
            document_info: vec![
                ("Author".to_owned(), "A".to_owned()),
                ("Title".to_owned(), "T".to_owned()),
            ],
            title: Some("T".to_owned()),
            author: Some("A".to_owned()),
            subject: None,
            keywords: None,
            creator: None,
            producer: None,
            creation_date_raw: None,
            creation_date_iso: None,
            modification_date_raw: None,
            modification_date_iso: None,
            version_header: Some("1.7".to_owned()),
            version_catalog: None,
            file_size_bytes: 1234,
            page_count: 2,
            encrypted: false,
            printable: true,
            modifiable: false,
            extractable: true,
            linearized: false,
            tagged: true,
            user_properties: false,
            suspects: false,
            form: "none".to_owned(),
            has_javascript: false,
            xmp_metadata: None,
        }
    }

    #[test]
    fn metadata_batch_keeps_absent_fields_absent() {
        let built = metadata_built(&metadata()).unwrap();
        let v = View(&built, &schema::SCHEMA);
        let ranges = v.ranges("Ranges");
        assert_eq!(ranges.len(), schema::SCHEMA.ranges.len());
        assert_eq!(ranges[0].as_deref(), Some("T"));
        assert_eq!(ranges[2], None);
        assert_eq!(
            ranges[schema::Range::Form as usize].as_deref(),
            Some("none")
        );
        assert_eq!(
            v.ranges("InfoKeys"),
            vec![Some("Author".to_owned()), Some("Title".to_owned())]
        );
        let expected = 1 << schema::Flag::Printable as u32
            | 1 << schema::Flag::Extractable as u32
            | 1 << schema::Flag::Tagged as u32;
        assert_eq!(v.int("Flags"), expected);
        assert_eq!(v.int("FileSize"), 1234);
    }

    #[test]
    fn pdf_date_preserves_partial_values_and_normalizes_offsets() {
        assert_eq!(pdf_date_to_iso("D:2026"), Some("2026".to_owned()));
        assert_eq!(
            pdf_date_to_iso("D:20260704114317+03'00'"),
            Some("2026-07-04T11:43:17+03:00".to_owned())
        );
    }
}
