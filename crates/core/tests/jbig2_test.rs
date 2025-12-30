//! Tests for JBIG2 segment reader/writer - port of pdfminer.six jbig2.py

use bolivar_core::jbig2::{Jbig2Segment, Jbig2StreamReader, Jbig2StreamWriter};
use std::io::Cursor;

// Constants for segment types
const SEG_TYPE_IMMEDIATE_GEN_REGION: u8 = 38;
const SEG_TYPE_END_OF_PAGE: u8 = 49;
const SEG_TYPE_END_OF_FILE: u8 = 51;

// File header
const FILE_HEADER_ID: &[u8] = b"\x97\x4a\x42\x32\x0d\x0a\x1a\x0a";

/// Build a simple segment header in memory
fn build_segment_header(
    number: u32,
    seg_type: u8,
    page_assoc: u8,
    data_length: u32,
    data: &[u8],
) -> Vec<u8> {
    let mut buf = Vec::new();
    // Segment number (4 bytes, big endian)
    buf.extend_from_slice(&number.to_be_bytes());
    // Flags (1 byte) - just segment type in lower 6 bits
    buf.push(seg_type & 0x3F);
    // Retention flags (1 byte) - ref_count=0
    buf.push(0x00);
    // Page association (1 byte)
    buf.push(page_assoc);
    // Data length (4 bytes, big endian)
    buf.extend_from_slice(&data_length.to_be_bytes());
    // Raw data
    buf.extend_from_slice(data);
    buf
}

// =============================================================================
// JBIG2StreamReader Tests
// =============================================================================

#[test]
fn test_reader_empty_stream() {
    let data: &[u8] = &[];
    let mut cursor = Cursor::new(data);
    let mut reader = Jbig2StreamReader::new(&mut cursor);
    let segments = reader.get_segments().unwrap();
    assert!(segments.is_empty());
}

#[test]
fn test_reader_single_segment() {
    let raw_data = b"test data";
    let segment_bytes = build_segment_header(
        1,
        SEG_TYPE_IMMEDIATE_GEN_REGION,
        1,
        raw_data.len() as u32,
        raw_data,
    );

    let mut cursor = Cursor::new(&segment_bytes);
    let mut reader = Jbig2StreamReader::new(&mut cursor);
    let segments = reader.get_segments().unwrap();

    assert_eq!(segments.len(), 1);
    let seg = &segments[0];
    assert_eq!(seg.number, 1);
    assert_eq!(seg.flags.seg_type, SEG_TYPE_IMMEDIATE_GEN_REGION);
    assert_eq!(seg.page_assoc, 1);
    assert_eq!(seg.data_length, raw_data.len() as u32);
    assert_eq!(seg.raw_data, raw_data);
}

#[test]
fn test_reader_multiple_segments() {
    let mut data = Vec::new();
    data.extend(build_segment_header(
        1,
        SEG_TYPE_IMMEDIATE_GEN_REGION,
        1,
        5,
        b"hello",
    ));
    data.extend(build_segment_header(
        2,
        SEG_TYPE_IMMEDIATE_GEN_REGION,
        1,
        5,
        b"world",
    ));
    data.extend(build_segment_header(3, SEG_TYPE_END_OF_PAGE, 1, 0, b""));

    let mut cursor = Cursor::new(&data);
    let mut reader = Jbig2StreamReader::new(&mut cursor);
    let segments = reader.get_segments().unwrap();

    assert_eq!(segments.len(), 3);
    assert_eq!(segments[0].number, 1);
    assert_eq!(segments[1].number, 2);
    assert_eq!(segments[2].number, 3);
    assert_eq!(segments[2].flags.seg_type, SEG_TYPE_END_OF_PAGE);
}

#[test]
fn test_reader_truncated_header() {
    // Only 5 bytes - not enough for full header
    let data: &[u8] = &[0x00, 0x00, 0x00, 0x01, 0x26];
    let mut cursor = Cursor::new(data);
    let mut reader = Jbig2StreamReader::new(&mut cursor);
    let segments = reader.get_segments().unwrap();
    // Should return empty or partial (depending on implementation)
    assert!(segments.is_empty());
}

#[test]
fn test_reader_deferred_flag() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&1u32.to_be_bytes()); // number
    buf.push(0x80 | SEG_TYPE_IMMEDIATE_GEN_REGION); // flags with deferred bit set
    buf.push(0x00); // retention flags
    buf.push(1); // page assoc
    buf.extend_from_slice(&0u32.to_be_bytes()); // data length

    let mut cursor = Cursor::new(&buf);
    let mut reader = Jbig2StreamReader::new(&mut cursor);
    let segments = reader.get_segments().unwrap();

    assert_eq!(segments.len(), 1);
    assert!(segments[0].flags.deferred);
}

#[test]
fn test_reader_page_assoc_long() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&1u32.to_be_bytes()); // number
    buf.push(0x40 | SEG_TYPE_IMMEDIATE_GEN_REGION); // flags with page_assoc_long bit set
    buf.push(0x00); // retention flags
    buf.extend_from_slice(&300u32.to_be_bytes()); // page assoc (4 bytes)
    buf.extend_from_slice(&0u32.to_be_bytes()); // data length

    let mut cursor = Cursor::new(&buf);
    let mut reader = Jbig2StreamReader::new(&mut cursor);
    let segments = reader.get_segments().unwrap();

    assert_eq!(segments.len(), 1);
    assert!(segments[0].flags.page_assoc_long);
    assert_eq!(segments[0].page_assoc, 300);
}

// =============================================================================
// JBIG2StreamWriter Tests
// =============================================================================

#[test]
fn test_writer_single_segment() {
    let segment = Jbig2Segment::new(1, SEG_TYPE_IMMEDIATE_GEN_REGION, 1, b"test".to_vec());

    let mut output = Vec::new();
    {
        let mut cursor = Cursor::new(&mut output);
        let mut writer = Jbig2StreamWriter::new(&mut cursor);
        writer.write_segments(&[segment], false).unwrap();
    }

    // Verify by reading back
    let mut cursor = Cursor::new(&output);
    let mut reader = Jbig2StreamReader::new(&mut cursor);
    let segments = reader.get_segments().unwrap();

    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].number, 1);
    assert_eq!(segments[0].raw_data, b"test");
}

#[test]
fn test_writer_file_header() {
    let segment = Jbig2Segment::new(1, SEG_TYPE_IMMEDIATE_GEN_REGION, 1, b"data".to_vec());

    let mut output = Vec::new();
    {
        let mut cursor = Cursor::new(&mut output);
        let mut writer = Jbig2StreamWriter::new(&mut cursor);
        writer.write_file(&[segment], false).unwrap();
    }

    // Check file header
    assert!(output.starts_with(FILE_HEADER_ID));
    // Check sequential flag
    assert_eq!(output[8], 0x01);
    // Check page count (1)
    assert_eq!(&output[9..13], &[0, 0, 0, 1]);
}

#[test]
fn test_writer_eof_segment() {
    let segment = Jbig2Segment::new(1, SEG_TYPE_IMMEDIATE_GEN_REGION, 1, b"data".to_vec());

    let mut output = Vec::new();
    {
        let mut cursor = Cursor::new(&mut output);
        let mut writer = Jbig2StreamWriter::new(&mut cursor);
        writer.write_file(&[segment], false).unwrap();
    }

    // Skip file header (13 bytes) and read segments
    let mut cursor = Cursor::new(&output[13..]);
    let mut reader = Jbig2StreamReader::new(&mut cursor);
    let segments = reader.get_segments().unwrap();

    // Should have original segment + EOF segment
    assert!(segments.len() >= 2);
    let last = segments.last().unwrap();
    assert_eq!(last.flags.seg_type, SEG_TYPE_END_OF_FILE);
}

#[test]
fn test_writer_fix_last_page() {
    let segment = Jbig2Segment::new(1, SEG_TYPE_IMMEDIATE_GEN_REGION, 1, b"data".to_vec());

    let mut output = Vec::new();
    {
        let mut cursor = Cursor::new(&mut output);
        let mut writer = Jbig2StreamWriter::new(&mut cursor);
        writer.write_file(&[segment], true).unwrap(); // fix_last_page = true
    }

    // Skip file header and read segments
    let mut cursor = Cursor::new(&output[13..]);
    let mut reader = Jbig2StreamReader::new(&mut cursor);
    let segments = reader.get_segments().unwrap();

    // Should have: original segment + EOP segment + EOF segment
    assert!(segments.len() >= 2);
    // Find EOP segment
    let has_eop = segments
        .iter()
        .any(|s| s.flags.seg_type == SEG_TYPE_END_OF_PAGE);
    assert!(
        has_eop,
        "Expected end-of-page segment when fix_last_page=true"
    );
}

// =============================================================================
// Round-trip Tests
// =============================================================================

#[test]
fn test_roundtrip_single_segment() {
    let original = Jbig2Segment::new(
        42,
        SEG_TYPE_IMMEDIATE_GEN_REGION,
        1,
        b"hello world".to_vec(),
    );

    // Write
    let mut output = Vec::new();
    {
        let mut cursor = Cursor::new(&mut output);
        let mut writer = Jbig2StreamWriter::new(&mut cursor);
        writer.write_segments(&[original.clone()], false).unwrap();
    }

    // Read back
    let mut cursor = Cursor::new(&output);
    let mut reader = Jbig2StreamReader::new(&mut cursor);
    let segments = reader.get_segments().unwrap();

    assert_eq!(segments.len(), 1);
    let read_back = &segments[0];
    assert_eq!(read_back.number, original.number);
    assert_eq!(read_back.flags.seg_type, original.flags.seg_type);
    assert_eq!(read_back.page_assoc, original.page_assoc);
    assert_eq!(read_back.raw_data, original.raw_data);
}

#[test]
fn test_roundtrip_multiple_segments() {
    let segments_in = vec![
        Jbig2Segment::new(1, SEG_TYPE_IMMEDIATE_GEN_REGION, 1, b"first".to_vec()),
        Jbig2Segment::new(2, SEG_TYPE_IMMEDIATE_GEN_REGION, 1, b"second".to_vec()),
        Jbig2Segment::new(3, SEG_TYPE_END_OF_PAGE, 1, vec![]),
    ];

    // Write
    let mut output = Vec::new();
    {
        let mut cursor = Cursor::new(&mut output);
        let mut writer = Jbig2StreamWriter::new(&mut cursor);
        writer.write_segments(&segments_in, false).unwrap();
    }

    // Read back
    let mut cursor = Cursor::new(&output);
    let mut reader = Jbig2StreamReader::new(&mut cursor);
    let segments_out = reader.get_segments().unwrap();

    assert_eq!(segments_out.len(), segments_in.len());
    for (orig, read) in segments_in.iter().zip(segments_out.iter()) {
        assert_eq!(read.number, orig.number);
        assert_eq!(read.flags.seg_type, orig.flags.seg_type);
        assert_eq!(read.raw_data, orig.raw_data);
    }
}

#[test]
fn test_roundtrip_file_format() {
    let segment = Jbig2Segment::new(1, SEG_TYPE_IMMEDIATE_GEN_REGION, 1, b"file test".to_vec());

    // Write as file
    let mut output = Vec::new();
    {
        let mut cursor = Cursor::new(&mut output);
        let mut writer = Jbig2StreamWriter::new(&mut cursor);
        writer.write_file(&[segment.clone()], false).unwrap();
    }

    // Verify header
    assert!(output.starts_with(FILE_HEADER_ID));

    // Read segments after header
    let mut cursor = Cursor::new(&output[13..]);
    let mut reader = Jbig2StreamReader::new(&mut cursor);
    let segments = reader.get_segments().unwrap();

    // First segment should match original
    assert!(!segments.is_empty());
    assert_eq!(segments[0].number, segment.number);
    assert_eq!(segments[0].raw_data, segment.raw_data);

    // Last segment should be EOF
    let last = segments.last().unwrap();
    assert_eq!(last.flags.seg_type, SEG_TYPE_END_OF_FILE);
}

#[test]
fn test_roundtrip_with_references() {
    // Segment with references to previous segments
    let mut segment = Jbig2Segment::new(5, SEG_TYPE_IMMEDIATE_GEN_REGION, 1, b"refs".to_vec());
    segment.retention_flags.ref_count = 2;
    segment.retention_flags.ref_segments = vec![1, 3];
    segment.retention_flags.retain_segments = vec![true, false];

    // Write
    let mut output = Vec::new();
    {
        let mut cursor = Cursor::new(&mut output);
        let mut writer = Jbig2StreamWriter::new(&mut cursor);
        writer.write_segments(&[segment.clone()], false).unwrap();
    }

    // Read back
    let mut cursor = Cursor::new(&output);
    let mut reader = Jbig2StreamReader::new(&mut cursor);
    let segments = reader.get_segments().unwrap();

    assert_eq!(segments.len(), 1);
    let read_back = &segments[0];
    assert_eq!(read_back.retention_flags.ref_count, 2);
    assert_eq!(read_back.retention_flags.ref_segments, vec![1, 3]);
}
