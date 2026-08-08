//! Tests for CMap database and Unicode mapping.
//!
//! Based on pdfminer.six cmapdb.py functionality.

use bolivar_core::cmapdb::{
    CMap, CMapBase, IdentityCMap, IdentityCMapByte, IdentityUnicodeMap, UnicodeMap,
};

// === IdentityCMap tests ===

#[test]
fn test_identity_cmap_h() {
    let cmap = IdentityCMap::new(false);
    assert!(!cmap.is_vertical());

    // 2-byte big-endian identity mapping
    let result: Vec<u32> = cmap.decode(&[0x00, 0x41]).collect();
    assert_eq!(result, vec![0x0041]); // 'A'
}

#[test]
fn test_identity_cmap_v() {
    let cmap = IdentityCMap::new(true);
    assert!(cmap.is_vertical());
}

#[test]
fn test_identity_cmap_multiple() {
    let cmap = IdentityCMap::new(false);
    let result: Vec<u32> = cmap.decode(&[0x00, 0x41, 0x00, 0x42]).collect();
    assert_eq!(result, vec![0x0041, 0x0042]); // 'AB'
}

#[test]
fn test_identity_cmap_empty() {
    let cmap = IdentityCMap::new(false);
    let result: Vec<u32> = cmap.decode(&[]).collect();
    assert!(result.is_empty());
}

#[test]
fn test_identity_cmap_odd_bytes() {
    let cmap = IdentityCMap::new(false);
    // Odd number of bytes - last byte ignored
    let result: Vec<u32> = cmap.decode(&[0x00, 0x41, 0x00]).collect();
    assert_eq!(result, vec![0x0041]);
}

// === IdentityCMapByte tests ===

#[test]
fn test_identity_cmap_byte_h() {
    let cmap = IdentityCMapByte::new(false);
    assert!(!cmap.is_vertical());

    // 1-byte identity mapping
    let result: Vec<u32> = cmap.decode(&[0x41, 0x42, 0x43]).collect();
    assert_eq!(result, vec![0x41, 0x42, 0x43]); // 'ABC'
}

#[test]
fn test_identity_cmap_byte_v() {
    let cmap = IdentityCMapByte::new(true);
    assert!(cmap.is_vertical());
}

// === CMap tests ===

#[test]
fn test_cmap_new() {
    let cmap = CMap::new();
    assert!(!cmap.is_vertical());
}

#[test]
fn test_cmap_vertical() {
    let mut cmap = CMap::new();
    cmap.set_vertical(true);
    assert!(cmap.is_vertical());
}

#[test]
fn test_cmap_add_code2cid_single_byte() {
    let mut cmap = CMap::new();
    cmap.add_code2cid(&[0x20], 1);
    cmap.add_code2cid(&[0x21], 2);

    let result: Vec<u32> = cmap.decode(&[0x20]).collect();
    assert_eq!(result, vec![1]);

    let result: Vec<u32> = cmap.decode(&[0x21]).collect();
    assert_eq!(result, vec![2]);
}

#[test]
fn test_cmap_add_code2cid_two_byte() {
    let mut cmap = CMap::new();
    cmap.add_code2cid(&[0x00, 0x41], 100);
    cmap.add_code2cid(&[0x00, 0x42], 101);

    let result: Vec<u32> = cmap.decode(&[0x00, 0x41]).collect();
    assert_eq!(result, vec![100]);
}

#[test]
fn test_cmap_add_cid_range() {
    let mut cmap = CMap::new();
    // Map 0x41-0x5A to CIDs 100-125 (A-Z)
    cmap.add_cid_range(&[0x41], &[0x5A], 100);

    let result: Vec<u32> = cmap.decode(&[0x41]).collect();
    assert_eq!(result, vec![100]); // A → 100

    let result: Vec<u32> = cmap.decode(&[0x42]).collect();
    assert_eq!(result, vec![101]); // B → 101

    let result: Vec<u32> = cmap.decode(&[0x5A]).collect();
    assert_eq!(result, vec![125]); // Z → 125
}

#[test]
fn test_cmap_unknown_code() {
    let cmap = CMap::new();
    // Unknown codes return nothing (empty iterator)
    let result: Vec<u32> = cmap.decode(&[0xFF]).collect();
    assert!(result.is_empty());
}

// === UnicodeMap tests ===

#[test]
fn test_unicode_map_new() {
    let umap = UnicodeMap::new();
    assert!(!umap.is_vertical());
}

#[test]
fn test_unicode_map_add_bf_char() {
    let mut umap = UnicodeMap::new();
    umap.add_cid2unichr(1, "A".to_string());
    umap.add_cid2unichr(2, "B".to_string());

    assert_eq!(umap.get_unichr(1), Some("A".to_string()));
    assert_eq!(umap.get_unichr(2), Some("B".to_string()));
    assert_eq!(umap.get_unichr(999), None);
}

#[test]
fn test_unicode_map_add_bf_range() {
    let mut umap = UnicodeMap::new();
    // Map CIDs 100-102 to "A", "B", "C" using UTF-16BE bytes
    // 'A' = U+0041 = [0x00, 0x41] in UTF-16BE
    umap.add_bf_range(100, 102, vec![0x00, 0x41]);

    assert_eq!(umap.get_unichr(100), Some("A".to_string()));
    assert_eq!(umap.get_unichr(101), Some("B".to_string()));
    assert_eq!(umap.get_unichr(102), Some("C".to_string()));
    assert_eq!(umap.get_unichr(103), None);
}

#[test]
fn test_unicode_map_utf16be() {
    let mut umap = UnicodeMap::new();
    // Add using UTF-16BE bytes (common in ToUnicode maps)
    umap.add_cid2unichr_bytes(1, &[0x00, 0x41]); // 'A'
    umap.add_cid2unichr_bytes(2, &[0x00, 0x42]); // 'B'

    assert_eq!(umap.get_unichr(1), Some("A".to_string()));
    assert_eq!(umap.get_unichr(2), Some("B".to_string()));
}

// === IdentityUnicodeMap tests ===

#[test]
fn test_identity_unicode_map() {
    let umap = IdentityUnicodeMap::new();

    // CID is interpreted directly as Unicode codepoint
    assert_eq!(umap.get_unichr(0x41), Some("A".to_string()));
    assert_eq!(umap.get_unichr(0x42), Some("B".to_string()));
    assert_eq!(umap.get_unichr(0x20AC), Some("€".to_string())); // Euro sign
}

// === CMap type detection tests (from test_pdfencoding.py) ===

#[test]
fn test_is_identity_cmap_name() {
    use bolivar_core::cmapdb::CMapDB;

    // Identity-H and Identity-V should return IdentityCMap
    assert!(CMapDB::is_identity_cmap("Identity-H"));
    assert!(CMapDB::is_identity_cmap("Identity-V"));
    assert!(CMapDB::is_identity_cmap("DLIdent-H"));
    assert!(CMapDB::is_identity_cmap("DLIdent-V"));
    assert!(!CMapDB::is_identity_cmap("UniGB-UCS2-H"));
}

#[test]
fn test_is_identity_cmap_byte_name() {
    use bolivar_core::cmapdb::CMapDB;

    assert!(CMapDB::is_identity_cmap_byte("OneByteIdentityH"));
    assert!(CMapDB::is_identity_cmap_byte("OneByteIdentityV"));
    assert!(!CMapDB::is_identity_cmap_byte("Identity-H"));
}

// === Additional IdentityCMap tests matching Python parity ===

#[test]
fn test_identity_cmap_odd_length_buffer() {
    // Python: struct.pack(">10H", *range(10)) + b"\x00" = 21 bytes
    // Decodes to (0, 1, 2, 3, 4, 5, 6, 7, 8, 9)
    let cmap = IdentityCMap::new(false);

    // Build buffer: 10 big-endian u16 shorts (0-9) + 1 extra byte
    let mut buffer: Vec<u8> = Vec::new();
    for i in 0u16..10 {
        buffer.extend_from_slice(&i.to_be_bytes());
    }
    buffer.push(0x00); // Extra byte to make it odd (21 bytes)

    assert_eq!(buffer.len(), 21);

    let result: Vec<u32> = cmap.decode(&buffer).collect();
    assert_eq!(result.len(), 10);
    assert_eq!(result, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
}

#[test]
fn test_identity_cmap_even_length_buffer() {
    // Python: struct.pack(">10H", *range(10)) = 20 bytes
    let cmap = IdentityCMap::new(false);

    let mut buffer: Vec<u8> = Vec::new();
    for i in 0u16..10 {
        buffer.extend_from_slice(&i.to_be_bytes());
    }

    assert_eq!(buffer.len(), 20);

    let result: Vec<u32> = cmap.decode(&buffer).collect();
    assert_eq!(result.len(), 10);
    assert_eq!(result, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
}

#[test]
fn test_identity_cmap_single_byte_buffer() {
    // Python: b"\x00" -> () (empty tuple since 1 // 2 = 0)
    let cmap = IdentityCMap::new(false);
    let result: Vec<u32> = cmap.decode(&[0x00]).collect();
    assert_eq!(result, Vec::<u32>::new());
}

#[test]
fn test_identity_cmap_various_odd_lengths() {
    // Python: Test various odd lengths 3, 5, 7, 9, 11, 13, 15, 17, 19, 21
    let cmap = IdentityCMap::new(false);

    for num_shorts in 1..=10 {
        let buffer_size = num_shorts * 2 + 1; // Add 1 to make it odd

        let mut buffer: Vec<u8> = Vec::new();
        for i in 0..num_shorts as u16 {
            buffer.extend_from_slice(&i.to_be_bytes());
        }
        buffer.push(0x00); // Extra byte

        assert_eq!(buffer.len(), buffer_size);

        let result: Vec<u32> = cmap.decode(&buffer).collect();
        assert_eq!(result.len(), num_shorts);

        let expected: Vec<u32> = (0..num_shorts as u32).collect();
        assert_eq!(result, expected);
    }
}

#[test]
fn test_identity_cmap_max_values() {
    // Python: max_values = [65535, 65534, 65533, 65532, 65531]
    let cmap = IdentityCMap::new(false);

    let max_values: Vec<u16> = vec![65535, 65534, 65533, 65532, 65531];
    let mut buffer: Vec<u8> = Vec::new();
    for v in &max_values {
        buffer.extend_from_slice(&v.to_be_bytes());
    }

    let result: Vec<u32> = cmap.decode(&buffer).collect();
    assert_eq!(result.len(), max_values.len());

    let expected: Vec<u32> = max_values.iter().map(|&v| v as u32).collect();
    assert_eq!(result, expected);
}

#[test]
fn test_identity_cmap_byte_odd_length_parametrized() {
    // Python: for length in [1, 5, 11, 13, 21, 255]
    let cmap = IdentityCMapByte::new(false);

    for length in [1usize, 5, 11, 13, 21, 255] {
        let test_buffer: Vec<u8> = (0u8..).take(length).collect();

        let result: Vec<u32> = cmap.decode(&test_buffer).collect();
        assert_eq!(result.len(), length, "Failed for length {}", length);

        let expected: Vec<u32> = (0u32..).take(length).collect();
        assert_eq!(result, expected, "Values mismatch for length {}", length);
    }
}
