//! PDF Page - represents a page in a PDF document.
//!
//! Port of pdfminer.six pdfpage.py

use super::catalog::PDFDocument;
use crate::error::{PdfError, Result};
use crate::model::objects::PDFObject;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[cfg(test)]
use std::sync::{Mutex, OnceLock};

#[cfg(test)]
static PAGE_CREATE_COUNTS: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();

#[cfg(test)]
fn page_create_counts() -> &'static Mutex<HashMap<usize, usize>> {
    PAGE_CREATE_COUNTS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(test)]
fn doc_key(doc: &PDFDocument) -> usize {
    doc as *const PDFDocument as usize
}

#[cfg(test)]
pub(crate) fn reset_page_create_count(doc: &PDFDocument) {
    let key = doc_key(doc);
    if let Ok(mut counts) = page_create_counts().lock() {
        counts.insert(key, 0);
    }
}

#[cfg(test)]
pub(crate) fn take_page_create_count(doc: &PDFDocument) -> usize {
    let key = doc_key(doc);
    if let Ok(mut counts) = page_create_counts().lock() {
        return counts.remove(&key).unwrap_or(0);
    }
    0
}

#[cfg(test)]
fn bump_page_create_count(doc: &PDFDocument) {
    let key = doc_key(doc);
    if let Ok(mut counts) = page_create_counts().lock() {
        let entry = counts.entry(key).or_insert(0);
        *entry += 1;
    }
}

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
    /// Create pages iterator from a PDFDocument.
    pub fn create_pages(doc: &PDFDocument) -> PageIterator<'_> {
        PageIterator::new(doc)
    }

    /// Get a single page by index using the cached page index.
    pub fn get_page_by_index(doc: &PDFDocument, index: usize) -> Result<Self> {
        let page_ref = doc
            .page_index()
            .get(index)
            .ok_or_else(|| PdfError::InvalidArgument("page index out of range".to_string()))?;

        let obj = doc.getobj_shared(page_ref.objid)?;
        let dict = obj
            .as_ref()
            .as_dict()
            .map_err(|_| PdfError::SyntaxError("Page object missing dict".into()))?;
        let mut attrs = dict.clone();
        if let Some(inherited) = &page_ref.inherited {
            inherited.apply_to(&mut attrs);
        }

        PDFPage::from_attrs(page_ref.objid, attrs, page_ref.label.clone(), doc)
    }

    /// Create a page from attributes.
    fn from_attrs(
        pageid: u32,
        attrs: HashMap<String, PDFObject>,
        label: Option<String>,
        doc: &PDFDocument,
    ) -> Result<Self> {
        #[cfg(test)]
        bump_page_create_count(doc);
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

        let resolved = match doc.resolve_shared(contents_obj) {
            Ok(obj) => obj,
            Err(_) => return Vec::new(),
        };

        // Contents can be a single stream or array of streams
        match resolved.as_ref() {
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
                    doc.resolve_shared(item).ok().and_then(|obj| {
                        obj.as_ref()
                            .as_stream()
                            .ok()
                            .and_then(|s| doc.decode_stream(s).ok())
                    })
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Get decoded content streams, parsing lazily if not already present.
    pub fn get_contents(&self, doc: &PDFDocument) -> Vec<Vec<u8>> {
        if self.contents.is_empty() {
            Self::parse_contents(&self.attrs, doc)
        } else {
            self.contents.clone()
        }
    }

    fn parse_box(
        attrs: &HashMap<String, PDFObject>,
        key: &str,
        doc: &PDFDocument,
    ) -> Option<[f64; 4]> {
        let obj = attrs.get(key)?;
        Self::parse_box_obj(obj, doc)
    }

    fn parse_box_obj(obj: &PDFObject, doc: &PDFDocument) -> Option<[f64; 4]> {
        let resolved = doc.resolve_shared(obj).ok()?;
        let arr = resolved.as_ref().as_array().ok()?;
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

#[derive(Debug)]
struct InheritedNode {
    parent: Option<Arc<InheritedNode>>,
    resources: Option<PDFObject>,
    mediabox: Option<PDFObject>,
    cropbox: Option<PDFObject>,
    rotate: Option<PDFObject>,
}

impl InheritedNode {
    fn from_dict(
        parent: Option<Arc<InheritedNode>>,
        dict: &HashMap<String, PDFObject>,
    ) -> Arc<Self> {
        Arc::new(Self {
            parent,
            resources: dict.get("Resources").cloned(),
            mediabox: dict.get("MediaBox").cloned(),
            cropbox: dict.get("CropBox").cloned(),
            rotate: dict.get("Rotate").cloned(),
        })
    }

    fn resolve_resources(&self) -> Option<&PDFObject> {
        self.resources.as_ref().or_else(|| {
            self.parent
                .as_ref()
                .and_then(|parent| parent.resolve_resources())
        })
    }

    fn resolve_mediabox(&self) -> Option<&PDFObject> {
        self.mediabox.as_ref().or_else(|| {
            self.parent
                .as_ref()
                .and_then(|parent| parent.resolve_mediabox())
        })
    }

    fn resolve_cropbox(&self) -> Option<&PDFObject> {
        self.cropbox.as_ref().or_else(|| {
            self.parent
                .as_ref()
                .and_then(|parent| parent.resolve_cropbox())
        })
    }

    fn resolve_rotate(&self) -> Option<&PDFObject> {
        self.rotate.as_ref().or_else(|| {
            self.parent
                .as_ref()
                .and_then(|parent| parent.resolve_rotate())
        })
    }

    fn apply_to(&self, dest: &mut HashMap<String, PDFObject>) {
        if !dest.contains_key("Resources") {
            if let Some(val) = self.resolve_resources() {
                dest.insert("Resources".to_string(), val.clone());
            }
        }
        if !dest.contains_key("MediaBox") {
            if let Some(val) = self.resolve_mediabox() {
                dest.insert("MediaBox".to_string(), val.clone());
            }
        }
        if !dest.contains_key("CropBox") {
            if let Some(val) = self.resolve_cropbox() {
                dest.insert("CropBox".to_string(), val.clone());
            }
        }
        if !dest.contains_key("Rotate") {
            if let Some(val) = self.resolve_rotate() {
                dest.insert("Rotate".to_string(), val.clone());
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PageRef {
    objid: u32,
    inherited: Option<Arc<InheritedNode>>,
    label: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct PageIndex {
    pages: Vec<PageRef>,
}

impl PageIndex {
    pub(crate) fn new(doc: &PDFDocument) -> Self {
        let mut pages = Vec::new();
        if let Some(page_tree_pages) = Self::collect_from_page_tree(doc) {
            pages = page_tree_pages;
        }
        if pages.is_empty() {
            pages = Self::collect_from_fallback(doc);
        }
        Self { pages }
    }

    pub(crate) fn get(&self, index: usize) -> Option<&PageRef> {
        self.pages.get(index)
    }

    pub(crate) fn len(&self) -> usize {
        self.pages.len()
    }

    pub(crate) fn mediaboxes(&self, doc: &PDFDocument) -> Result<Vec<[f64; 4]>> {
        let mut boxes = Vec::with_capacity(self.pages.len());
        for (idx, page_ref) in self.pages.iter().enumerate() {
            let obj = doc.getobj_shared(page_ref.objid)?;
            let dict = obj
                .as_ref()
                .as_dict()
                .map_err(|_| PdfError::SyntaxError("Page object missing dict".into()))?;
            let mut mediabox_obj = dict.get("MediaBox");
            if mediabox_obj.is_none() {
                if let Some(inherited) = &page_ref.inherited {
                    mediabox_obj = inherited.resolve_mediabox();
                }
            }
            let mediabox = mediabox_obj
                .and_then(|obj| PDFPage::parse_box_obj(obj, doc))
                .ok_or_else(|| PdfError::SyntaxError(format!("Page {} missing mediabox", idx)))?;
            boxes.push(mediabox);
        }
        Ok(boxes)
    }

    fn labels_iter(doc: &PDFDocument) -> Option<Box<dyn Iterator<Item = String> + '_>> {
        doc.get_page_labels()
            .ok()
            .map(|l| Box::new(l) as Box<dyn Iterator<Item = String>>)
    }

    fn next_label(labels: &mut Option<Box<dyn Iterator<Item = String> + '_>>) -> Option<String> {
        labels.as_mut().and_then(|l| l.next())
    }

    fn collect_from_page_tree(doc: &PDFDocument) -> Option<Vec<PageRef>> {
        let catalog = doc.catalog();
        let pages_ref = catalog.get("Pages")?;
        let pages_ref = pages_ref.as_ref().ok()?;
        let mut labels = Self::labels_iter(doc);
        let mut stack = vec![(pages_ref.objid, InheritedNode::from_dict(None, catalog))];
        let mut visited = HashSet::new();
        let mut pages = Vec::new();

        while let Some((objid, parent_inherited)) = stack.pop() {
            if visited.contains(&objid) {
                continue;
            }
            visited.insert(objid);

            let obj = match doc.getobj_shared(objid) {
                Ok(o) => o,
                Err(_) => continue,
            };
            let dict = match obj.as_ref().as_dict() {
                Ok(d) => d,
                Err(_) => continue,
            };
            let obj_type = dict.get("Type").or_else(|| dict.get("type"));

            match obj_type {
                Some(PDFObject::Name(name)) if name == "Pages" => {
                    let inherited =
                        InheritedNode::from_dict(Some(Arc::clone(&parent_inherited)), dict);
                    if let Some(kids) = dict.get("Kids")
                        && let Ok(kids) = doc.resolve(kids)
                        && let Ok(kids_arr) = kids.as_array()
                    {
                        for kid in kids_arr.iter().rev() {
                            if let Ok(kid_ref) = kid.as_ref() {
                                stack.push((kid_ref.objid, Arc::clone(&inherited)));
                            } else if let Ok(kid_int) = kid.as_int() {
                                stack.push((kid_int as u32, Arc::clone(&inherited)));
                            }
                        }
                    }
                }
                Some(PDFObject::Name(name)) if name == "Page" => {
                    pages.push(PageRef {
                        objid,
                        inherited: Some(parent_inherited),
                        label: Self::next_label(&mut labels),
                    });
                }
                _ => {}
            }
        }

        Some(pages)
    }

    fn collect_from_fallback(doc: &PDFDocument) -> Vec<PageRef> {
        let mut labels = Self::labels_iter(doc);
        let mut pages = Vec::new();
        for objid in doc.get_objids() {
            if let Ok(obj) = doc.getobj_shared(objid)
                && let Ok(dict) = obj.as_ref().as_dict()
                && let Some(PDFObject::Name(type_name)) = dict.get("Type")
                && type_name == "Page"
            {
                pages.push(PageRef {
                    objid,
                    inherited: None,
                    label: Self::next_label(&mut labels),
                });
            }
        }
        pages
    }
}

/// Iterator over pages in a PDF document.
pub struct PageIterator<'a> {
    doc: &'a PDFDocument,
    /// Stack for depth-first traversal: (objid, inherited_attrs)
    stack: Vec<(u32, Arc<InheritedNode>)>,
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
        if let Some(pages_ref) = catalog.get("Pages")
            && let Ok(pages_ref) = pages_ref.as_ref()
        {
            let inherited = InheritedNode::from_dict(None, catalog);
            return Self {
                doc,
                stack: vec![(pages_ref.objid, inherited)],
                visited: HashSet::new(),
                labels,
                fallback_mode: false,
                fallback_objids: doc.get_objids(),
                fallback_idx: 0,
                pages_yielded: false,
            };
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

                if let Ok(obj) = self.doc.getobj_shared(objid)
                    && let Ok(dict) = obj.as_ref().as_dict()
                    && let Some(PDFObject::Name(type_name)) = dict.get("Type")
                    && type_name == "Page"
                {
                    let label = self.get_next_label();
                    return Some(PDFPage::from_attrs(objid, dict.clone(), label, self.doc));
                }
            }
            return None;
        }

        // Normal mode: depth-first traversal of page tree
        while let Some((objid, parent_inherited)) = self.stack.pop() {
            if self.visited.contains(&objid) {
                continue;
            }
            self.visited.insert(objid);

            let obj = match self.doc.getobj_shared(objid) {
                Ok(o) => o,
                Err(_) => continue,
            };

            let dict = match obj.as_ref().as_dict() {
                Ok(d) => d,
                Err(_) => continue,
            };

            // Check Type
            let obj_type = dict.get("Type").or_else(|| dict.get("type"));

            match obj_type {
                Some(PDFObject::Name(name)) if name == "Pages" => {
                    // Intermediate node - push kids onto stack (in reverse for correct order)
                    let inherited =
                        InheritedNode::from_dict(Some(Arc::clone(&parent_inherited)), dict);
                    if let Some(kids) = dict.get("Kids")
                        && let Ok(kids) = self.doc.resolve(kids)
                        && let Ok(kids_arr) = kids.as_array()
                    {
                        for kid in kids_arr.iter().rev() {
                            if let Ok(kid_ref) = kid.as_ref() {
                                self.stack.push((kid_ref.objid, Arc::clone(&inherited)));
                            } else if let Ok(kid_int) = kid.as_int() {
                                // Sometimes kids are stored as integers
                                self.stack.push((kid_int as u32, Arc::clone(&inherited)));
                            }
                        }
                    }
                }
                Some(PDFObject::Name(name)) if name == "Page" => {
                    // Leaf node - this is a page
                    let mut attrs = dict.clone();
                    parent_inherited.apply_to(&mut attrs);
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

#[cfg(test)]
mod tests {
    use super::InheritedNode;
    use crate::model::objects::PDFObject;
    use std::collections::HashMap;

    #[test]
    fn test_inherited_node_apply_to_fills_missing() {
        let mut root = HashMap::new();
        root.insert("MediaBox".to_string(), PDFObject::Name("root".into()));
        root.insert("Rotate".to_string(), PDFObject::Int(90));

        let mut mid = HashMap::new();
        mid.insert("Resources".to_string(), PDFObject::Name("mid".into()));

        let root_node = InheritedNode::from_dict(None, &root);
        let mid_node = InheritedNode::from_dict(Some(root_node), &mid);

        let mut leaf = HashMap::new();
        leaf.insert("Resources".to_string(), PDFObject::Name("leaf".into()));

        mid_node.apply_to(&mut leaf);

        assert_eq!(leaf.get("Resources"), Some(&PDFObject::Name("leaf".into())));
        assert_eq!(leaf.get("MediaBox"), Some(&PDFObject::Name("root".into())));
        assert_eq!(leaf.get("Rotate"), Some(&PDFObject::Int(90)));
    }
}
