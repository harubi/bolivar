//! PDF Page - represents a page in a PDF document.
//!
//! Port of pdfminer.six pdfpage.py

use crate::error::Result;
use crate::pdfdocument::PDFDocument;
use crate::pdftypes::PDFObject;
use std::collections::{HashMap, HashSet};

/// A PDF page object.
#[derive(Debug)]
pub struct PDFPage {
    /// Page object ID
    pub pageid: u32,
    /// Page attributes dictionary
    pub attrs: HashMap<String, PDFObject>,
    /// Page label (logical page number)
    pub label: Option<String>,
    /// Media box (physical page size)
    pub mediabox: Option<[f64; 4]>,
    /// Crop box
    pub cropbox: Option<[f64; 4]>,
    /// Bleed box (printing bleed area)
    pub bleedbox: Option<[f64; 4]>,
    /// Trim box (finished page size after trimming)
    pub trimbox: Option<[f64; 4]>,
    /// Art box (meaningful content area)
    pub artbox: Option<[f64; 4]>,
    /// Page rotation in degrees
    pub rotate: i64,
    /// Page annotations
    pub annots: Option<PDFObject>,
    /// Page resources
    pub resources: HashMap<String, PDFObject>,
    /// Page content streams (decoded data)
    pub contents: Vec<Vec<u8>>,
    /// User unit (PDF 1.6) - scales default user space units. Default is 1.0.
    pub user_unit: f64,
}

impl PDFPage {
    /// Inheritable page attributes.
    const INHERITABLE_ATTRS: &'static [&'static str] =
        &["Resources", "MediaBox", "CropBox", "Rotate"];

    /// Create pages iterator from a PDFDocument.
    pub fn create_pages(doc: &PDFDocument) -> PageIterator<'_> {
        PageIterator::new(doc)
    }

    /// Create a page from attributes.
    fn from_attrs(
        pageid: u32,
        attrs: HashMap<String, PDFObject>,
        label: Option<String>,
        doc: &PDFDocument,
    ) -> Result<Self> {
        let mediabox = Self::parse_box(&attrs, "MediaBox", doc)
            .ok_or_else(|| crate::error::PdfError::SyntaxError("MediaBox missing".into()))?;
        let cropbox = Self::parse_box(&attrs, "CropBox", doc).or(Some(mediabox));
        let bleedbox = Self::parse_box(&attrs, "BleedBox", doc);
        let trimbox = Self::parse_box(&attrs, "TrimBox", doc);
        let artbox = Self::parse_box(&attrs, "ArtBox", doc);
        let rotate = attrs
            .get("Rotate")
            .and_then(|r| r.as_int().ok())
            .map(|r| (r + 360) % 360)
            .unwrap_or(0);
        let user_unit = attrs
            .get("UserUnit")
            .and_then(|u| u.as_num().ok())
            .unwrap_or(1.0);
        let annots = attrs.get("Annots").cloned();
        let resources = attrs
            .get("Resources")
            .and_then(|r| doc.resolve(r).ok())
            .and_then(|r| r.as_dict().ok().cloned())
            .unwrap_or_default();
        let contents = Vec::new();

        Ok(Self {
            pageid,
            attrs,
            label,
            mediabox: Some(mediabox),
            cropbox,
            bleedbox,
            trimbox,
            artbox,
            rotate,
            annots,
            resources,
            contents,
            user_unit,
        })
    }

    /// Parse page contents (content streams).
    ///
    /// Contents can be a single stream or an array of streams.
    /// Returns decoded data from all content streams.
    pub(crate) fn parse_contents(
        attrs: &HashMap<String, PDFObject>,
        doc: &PDFDocument,
    ) -> Vec<Vec<u8>> {
        let contents_obj = match attrs.get("Contents") {
            Some(obj) => obj,
            None => return Vec::new(),
        };

        let resolved = match doc.resolve(contents_obj) {
            Ok(obj) => obj,
            Err(_) => return Vec::new(),
        };

        // Contents can be a single stream or array of streams
        match &resolved {
            PDFObject::Stream(stream) => {
                // Decode the stream (handles FlateDecode, etc.)
                match doc.decode_stream(stream) {
                    Ok(data) => vec![data],
                    Err(_) => Vec::new(),
                }
            }
            PDFObject::Array(arr) => arr
                .iter()
                .filter_map(|item| {
                    doc.resolve(item).ok().and_then(|obj| {
                        obj.as_stream().ok().and_then(|s| doc.decode_stream(s).ok())
                    })
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    fn parse_box(
        attrs: &HashMap<String, PDFObject>,
        key: &str,
        doc: &PDFDocument,
    ) -> Option<[f64; 4]> {
        let obj = attrs.get(key)?;
        let resolved = doc.resolve(obj).ok()?;
        let arr = resolved.as_array().ok()?;
        if arr.len() != 4 {
            return None;
        }
        Some([
            arr[0].as_num().ok()?,
            arr[1].as_num().ok()?,
            arr[2].as_num().ok()?,
            arr[3].as_num().ok()?,
        ])
    }
}

/// Iterator over pages in a PDF document.
pub struct PageIterator<'a> {
    doc: &'a PDFDocument,
    /// Stack for depth-first traversal: (objid, inherited_attrs)
    stack: Vec<(u32, HashMap<String, PDFObject>)>,
    /// Visited object IDs (to prevent cycles)
    visited: HashSet<u32>,
    /// Page labels iterator
    labels: Option<Box<dyn Iterator<Item = String> + 'a>>,
    /// Whether we're in fallback mode (no /Pages)
    fallback_mode: bool,
    /// Fallback: list of object IDs to check
    fallback_objids: Vec<u32>,
    /// Fallback: current index
    fallback_idx: usize,
    /// Whether we've yielded any pages from the /Pages tree
    pages_yielded: bool,
}

impl<'a> PageIterator<'a> {
    fn new(doc: &'a PDFDocument) -> Self {
        // Try to get page labels
        let labels: Option<Box<dyn Iterator<Item = String> + 'a>> = doc
            .get_page_labels()
            .ok()
            .map(|l| Box::new(l) as Box<dyn Iterator<Item = String>>);

        let catalog = doc.catalog();

        // Check if we have a Pages reference
        if let Some(pages_ref) = catalog.get("Pages") {
            if let Ok(pages_ref) = pages_ref.as_ref() {
                return Self {
                    doc,
                    stack: vec![(pages_ref.objid, catalog.clone())],
                    visited: HashSet::new(),
                    labels,
                    fallback_mode: false,
                    fallback_objids: doc.get_objids(),
                    fallback_idx: 0,
                    pages_yielded: false,
                };
            }
        }

        // Fallback mode - no valid /Pages
        Self {
            doc,
            stack: Vec::new(),
            visited: HashSet::new(),
            labels,
            fallback_mode: true,
            fallback_objids: doc.get_objids(),
            fallback_idx: 0,
            pages_yielded: false,
        }
    }

    fn get_next_label(&mut self) -> Option<String> {
        self.labels.as_mut().and_then(|l| l.next())
    }
}

impl<'a> Iterator for PageIterator<'a> {
    type Item = Result<PDFPage>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.fallback_mode {
            // Fallback: iterate through all objects looking for Page type
            while self.fallback_idx < self.fallback_objids.len() {
                let objid = self.fallback_objids[self.fallback_idx];
                self.fallback_idx += 1;

                if let Ok(obj) = self.doc.getobj(objid) {
                    if let Ok(dict) = obj.as_dict() {
                        if let Some(PDFObject::Name(type_name)) = dict.get("Type") {
                            if type_name == "Page" {
                                let label = self.get_next_label();
                                return Some(PDFPage::from_attrs(
                                    objid,
                                    dict.clone(),
                                    label,
                                    self.doc,
                                ));
                            }
                        }
                    }
                }
            }
            return None;
        }

        // Normal mode: depth-first traversal of page tree
        while let Some((objid, parent_attrs)) = self.stack.pop() {
            if self.visited.contains(&objid) {
                continue;
            }
            self.visited.insert(objid);

            let obj = match self.doc.getobj(objid) {
                Ok(o) => o,
                Err(_) => continue,
            };

            let dict = match obj.as_dict() {
                Ok(d) => d.clone(),
                Err(_) => continue,
            };

            // Merge inheritable attributes
            let mut attrs = dict.clone();
            for &key in PDFPage::INHERITABLE_ATTRS {
                if !attrs.contains_key(key) {
                    if let Some(val) = parent_attrs.get(key) {
                        attrs.insert(key.to_string(), val.clone());
                    }
                }
            }

            // Check Type
            let obj_type = dict.get("Type").or_else(|| dict.get("type"));

            match obj_type {
                Some(PDFObject::Name(name)) if name == "Pages" => {
                    // Intermediate node - push kids onto stack (in reverse for correct order)
                    if let Some(kids) = dict.get("Kids") {
                        if let Ok(kids) = self.doc.resolve(kids) {
                            if let Ok(kids_arr) = kids.as_array() {
                                for kid in kids_arr.iter().rev() {
                                    if let Ok(kid_ref) = kid.as_ref() {
                                        self.stack.push((kid_ref.objid, attrs.clone()));
                                    } else if let Ok(kid_int) = kid.as_int() {
                                        // Sometimes kids are stored as integers
                                        self.stack.push((kid_int as u32, attrs.clone()));
                                    }
                                }
                            }
                        }
                    }
                }
                Some(PDFObject::Name(name)) if name == "Page" => {
                    // Leaf node - this is a page
                    let label = self.get_next_label();
                    self.pages_yielded = true;
                    return Some(PDFPage::from_attrs(objid, attrs, label, self.doc));
                }
                _ => {
                    // Unknown type, skip
                }
            }
        }

        if !self.pages_yielded && !self.fallback_mode {
            self.fallback_mode = true;
            return self.next();
        }

        None
    }
}
