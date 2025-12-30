//! Tests for PDF data structures.
//!
//! Based on pdfminer.six data_structures.py functionality.

use bolivar_core::data_structures::NumberTree;
use bolivar_core::pdftypes::PDFObject;
use std::collections::HashMap;

/// Helper to create a dict PDFObject
fn make_dict(pairs: Vec<(&str, PDFObject)>) -> PDFObject {
    let mut map = HashMap::new();
    for (k, v) in pairs {
        map.insert(k.to_string(), v);
    }
    PDFObject::Dict(map)
}

// === NumberTree tests ===

#[test]
fn test_number_tree_simple_nums() {
    // Simple NumberTree with Nums array: [0, "a", 1, "b", 2, "c"]
    let obj = make_dict(vec![(
        "Nums",
        PDFObject::Array(vec![
            PDFObject::Int(0),
            PDFObject::Name("a".to_string()),
            PDFObject::Int(1),
            PDFObject::Name("b".to_string()),
            PDFObject::Int(2),
            PDFObject::Name("c".to_string()),
        ]),
    )]);

    let tree = NumberTree::new(&obj).unwrap();
    let values = tree.values();

    assert_eq!(values.len(), 3);
    assert_eq!(values[0].0, 0);
    assert_eq!(values[0].1.as_name().unwrap(), "a");
    assert_eq!(values[1].0, 1);
    assert_eq!(values[1].1.as_name().unwrap(), "b");
    assert_eq!(values[2].0, 2);
    assert_eq!(values[2].1.as_name().unwrap(), "c");
}

#[test]
fn test_number_tree_out_of_order() {
    // NumberTree with out-of-order Nums - should be sorted
    let obj = make_dict(vec![(
        "Nums",
        PDFObject::Array(vec![
            PDFObject::Int(5),
            PDFObject::Name("five".to_string()),
            PDFObject::Int(1),
            PDFObject::Name("one".to_string()),
            PDFObject::Int(3),
            PDFObject::Name("three".to_string()),
        ]),
    )]);

    let tree = NumberTree::new(&obj).unwrap();
    let values = tree.values();

    assert_eq!(values.len(), 3);
    // Should be sorted by number
    assert_eq!(values[0].0, 1);
    assert_eq!(values[0].1.as_name().unwrap(), "one");
    assert_eq!(values[1].0, 3);
    assert_eq!(values[1].1.as_name().unwrap(), "three");
    assert_eq!(values[2].0, 5);
    assert_eq!(values[2].1.as_name().unwrap(), "five");
}

#[test]
fn test_number_tree_with_kids() {
    // NumberTree with Kids - recursive structure
    // Child 1: Nums [0, "first", 1, "second"]
    let child1 = make_dict(vec![(
        "Nums",
        PDFObject::Array(vec![
            PDFObject::Int(0),
            PDFObject::Name("first".to_string()),
            PDFObject::Int(1),
            PDFObject::Name("second".to_string()),
        ]),
    )]);

    // Child 2: Nums [2, "third", 3, "fourth"]
    let child2 = make_dict(vec![(
        "Nums",
        PDFObject::Array(vec![
            PDFObject::Int(2),
            PDFObject::Name("third".to_string()),
            PDFObject::Int(3),
            PDFObject::Name("fourth".to_string()),
        ]),
    )]);

    // Root with Kids
    let root = make_dict(vec![("Kids", PDFObject::Array(vec![child1, child2]))]);

    let tree = NumberTree::new(&root).unwrap();
    let values = tree.values();

    assert_eq!(values.len(), 4);
    assert_eq!(values[0].0, 0);
    assert_eq!(values[0].1.as_name().unwrap(), "first");
    assert_eq!(values[1].0, 1);
    assert_eq!(values[1].1.as_name().unwrap(), "second");
    assert_eq!(values[2].0, 2);
    assert_eq!(values[2].1.as_name().unwrap(), "third");
    assert_eq!(values[3].0, 3);
    assert_eq!(values[3].1.as_name().unwrap(), "fourth");
}

#[test]
fn test_number_tree_nested_kids() {
    // Three-level deep structure: root -> intermediate -> leaves
    let leaf1 = make_dict(vec![(
        "Nums",
        PDFObject::Array(vec![PDFObject::Int(0), PDFObject::Name("zero".to_string())]),
    )]);

    let leaf2 = make_dict(vec![(
        "Nums",
        PDFObject::Array(vec![PDFObject::Int(1), PDFObject::Name("one".to_string())]),
    )]);

    let intermediate = make_dict(vec![("Kids", PDFObject::Array(vec![leaf1, leaf2]))]);

    let root = make_dict(vec![("Kids", PDFObject::Array(vec![intermediate]))]);

    let tree = NumberTree::new(&root).unwrap();
    let values = tree.values();

    assert_eq!(values.len(), 2);
    assert_eq!(values[0].0, 0);
    assert_eq!(values[0].1.as_name().unwrap(), "zero");
    assert_eq!(values[1].0, 1);
    assert_eq!(values[1].1.as_name().unwrap(), "one");
}

#[test]
fn test_number_tree_mixed_nums_and_kids() {
    // Tree with both Nums and Kids at the same level
    let child = make_dict(vec![(
        "Nums",
        PDFObject::Array(vec![PDFObject::Int(10), PDFObject::Name("ten".to_string())]),
    )]);

    let root = make_dict(vec![
        (
            "Nums",
            PDFObject::Array(vec![PDFObject::Int(0), PDFObject::Name("zero".to_string())]),
        ),
        ("Kids", PDFObject::Array(vec![child])),
    ]);

    let tree = NumberTree::new(&root).unwrap();
    let values = tree.values();

    // Should have both: 0 from Nums, 10 from Kids
    assert_eq!(values.len(), 2);
    assert_eq!(values[0].0, 0);
    assert_eq!(values[0].1.as_name().unwrap(), "zero");
    assert_eq!(values[1].0, 10);
    assert_eq!(values[1].1.as_name().unwrap(), "ten");
}

#[test]
fn test_number_tree_empty() {
    // Empty NumberTree (no Nums, no Kids)
    let obj = make_dict(vec![]);

    let tree = NumberTree::new(&obj).unwrap();
    let values = tree.values();

    assert!(values.is_empty());
}

#[test]
fn test_number_tree_empty_nums() {
    // NumberTree with empty Nums array
    let obj = make_dict(vec![("Nums", PDFObject::Array(vec![]))]);

    let tree = NumberTree::new(&obj).unwrap();
    let values = tree.values();

    assert!(values.is_empty());
}

#[test]
fn test_number_tree_empty_kids() {
    // NumberTree with empty Kids array
    let obj = make_dict(vec![("Kids", PDFObject::Array(vec![]))]);

    let tree = NumberTree::new(&obj).unwrap();
    let values = tree.values();

    assert!(values.is_empty());
}

#[test]
fn test_number_tree_invalid_not_dict() {
    // NumberTree must be a dict
    let obj = PDFObject::Array(vec![]);
    let result = NumberTree::new(&obj);
    assert!(result.is_err());
}

#[test]
fn test_number_tree_with_dict_values() {
    // NumberTree with dictionary values (like page label dicts)
    let label_dict = make_dict(vec![
        ("S", PDFObject::Name("r".to_string())), // Roman numerals
        ("St", PDFObject::Int(1)),               // Start at 1
    ]);

    let obj = make_dict(vec![(
        "Nums",
        PDFObject::Array(vec![PDFObject::Int(0), label_dict]),
    )]);

    let tree = NumberTree::new(&obj).unwrap();
    let values = tree.values();

    assert_eq!(values.len(), 1);
    assert_eq!(values[0].0, 0);
    let inner_dict = values[0].1.as_dict().unwrap();
    assert_eq!(inner_dict.get("S").unwrap().as_name().unwrap(), "r");
    assert_eq!(inner_dict.get("St").unwrap().as_int().unwrap(), 1);
}

#[test]
fn test_number_tree_with_limits() {
    // NumberTree with Limits (min/max bounds) - should still work
    let obj = make_dict(vec![
        (
            "Limits",
            PDFObject::Array(vec![PDFObject::Int(0), PDFObject::Int(5)]),
        ),
        (
            "Nums",
            PDFObject::Array(vec![
                PDFObject::Int(0),
                PDFObject::Name("start".to_string()),
                PDFObject::Int(5),
                PDFObject::Name("end".to_string()),
            ]),
        ),
    ]);

    let tree = NumberTree::new(&obj).unwrap();
    let values = tree.values();

    assert_eq!(values.len(), 2);
    assert_eq!(values[0].0, 0);
    assert_eq!(values[1].0, 5);
}
