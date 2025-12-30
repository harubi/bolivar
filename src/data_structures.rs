//! PDF data structures.
//!
//! Port of pdfminer.six data_structures.py

use crate::error::Result;
use crate::pdftypes::PDFObject;

/// A PDF number tree.
///
/// See Section 3.8.6 of the PDF Reference.
/// Used for page labels, structure element parent trees, etc.
pub struct NumberTree<'a> {
    obj: &'a PDFObject,
}

impl<'a> NumberTree<'a> {
    /// Create a new NumberTree from a PDF dictionary object.
    pub fn new(obj: &'a PDFObject) -> Result<Self> {
        // Validate that it's a dictionary
        obj.as_dict()?;
        Ok(Self { obj })
    }

    /// Parse the tree recursively, collecting (number, value) pairs.
    fn parse(&self) -> Result<Vec<(i64, PDFObject)>> {
        let mut items = Vec::new();
        let dict = self.obj.as_dict()?;

        // Process Nums array (leaf node)
        if let Some(nums_obj) = dict.get("Nums") {
            let nums = nums_obj.as_array()?;
            // Process pairs: [num1, val1, num2, val2, ...]
            for chunk in nums.chunks(2) {
                if chunk.len() == 2 {
                    let num = chunk[0].as_int()?;
                    let val = chunk[1].clone();
                    items.push((num, val));
                }
            }
        }

        // Process Kids array (intermediate/root node)
        if let Some(kids_obj) = dict.get("Kids") {
            let kids = kids_obj.as_array()?;
            for child in kids {
                let child_tree = NumberTree::new(child)?;
                items.extend(child_tree.parse()?);
            }
        }

        Ok(items)
    }

    /// Get all (number, value) pairs from the tree, sorted by number.
    ///
    /// Returns empty vec if parsing fails (lenient mode, matching pdfminer behavior).
    /// Silently drops trailing element if Nums array has odd length.
    pub fn values(&self) -> Vec<(i64, PDFObject)> {
        let mut values = self.parse().unwrap_or_default();
        // Sort by number (non-strict mode behavior from pdfminer)
        values.sort_by_key(|(num, _)| *num);
        values
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_dict(pairs: Vec<(&str, PDFObject)>) -> PDFObject {
        let mut map = HashMap::new();
        for (k, v) in pairs {
            map.insert(k.to_string(), v);
        }
        PDFObject::Dict(map)
    }

    #[test]
    fn test_basic_number_tree() {
        let obj = make_dict(vec![(
            "Nums",
            PDFObject::Array(vec![PDFObject::Int(0), PDFObject::Name("zero".to_string())]),
        )]);

        let tree = NumberTree::new(&obj).unwrap();
        let values = tree.values();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].0, 0);
    }
}
