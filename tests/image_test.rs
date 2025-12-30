//! Tests for BMP/image writing - port of pdfminer.six image.py

use bolivar::image::{BmpWriter, align32};
use std::io::Cursor;

// =============================================================================
// align32 Tests
// =============================================================================

#[test]
fn test_align32_zero() {
    assert_eq!(align32(0), 0);
}

#[test]
fn test_align32_already_aligned() {
    assert_eq!(align32(4), 4);
    assert_eq!(align32(8), 8);
    assert_eq!(align32(32), 32);
}

#[test]
fn test_align32_needs_padding() {
    assert_eq!(align32(1), 4);
    assert_eq!(align32(2), 4);
    assert_eq!(align32(3), 4);
    assert_eq!(align32(5), 8);
    assert_eq!(align32(6), 8);
    assert_eq!(align32(7), 8);
    assert_eq!(align32(9), 12);
}

// =============================================================================
// BmpWriter Tests - Header Structure
// =============================================================================

#[test]
fn test_bmpwriter_1bit_header() {
    let mut output = Vec::new();
    {
        let mut cursor = Cursor::new(&mut output);
        let _writer = BmpWriter::new(&mut cursor, 1, 8, 4).unwrap();
    }

    // BMP signature
    assert_eq!(&output[0..2], b"BM");

    // Header size is 14 + 40 + (ncols * 4) = 14 + 40 + 8 = 62
    // Data size for 1-bit, width=8, height=4: linesize = align32((8*1+7)/8) = align32(1) = 4
    // datasize = 4 * 4 = 16
    // Total file size = 62 + 16 = 78
    let file_size = u32::from_le_bytes([output[2], output[3], output[4], output[5]]);
    assert_eq!(file_size, 78);

    // Data offset should be 62
    let data_offset = u32::from_le_bytes([output[10], output[11], output[12], output[13]]);
    assert_eq!(data_offset, 62);
}

#[test]
fn test_bmpwriter_8bit_header() {
    let mut output = Vec::new();
    {
        let mut cursor = Cursor::new(&mut output);
        let _writer = BmpWriter::new(&mut cursor, 8, 10, 5).unwrap();
    }

    // BMP signature
    assert_eq!(&output[0..2], b"BM");

    // Header size is 14 + 40 + (256 * 4) = 14 + 40 + 1024 = 1078
    // Data size for 8-bit, width=10, height=5: linesize = align32((10*8+7)/8) = align32(10) = 12
    // datasize = 12 * 5 = 60
    // Total file size = 1078 + 60 = 1138
    let file_size = u32::from_le_bytes([output[2], output[3], output[4], output[5]]);
    assert_eq!(file_size, 1138);

    // Data offset should be 1078
    let data_offset = u32::from_le_bytes([output[10], output[11], output[12], output[13]]);
    assert_eq!(data_offset, 1078);
}

#[test]
fn test_bmpwriter_24bit_header() {
    let mut output = Vec::new();
    {
        let mut cursor = Cursor::new(&mut output);
        let _writer = BmpWriter::new(&mut cursor, 24, 10, 5).unwrap();
    }

    // BMP signature
    assert_eq!(&output[0..2], b"BM");

    // Header size is 14 + 40 + 0 = 54 (no color table for 24-bit)
    // Data size for 24-bit, width=10, height=5: linesize = align32((10*24+7)/8) = align32(30) = 32
    // datasize = 32 * 5 = 160
    // Total file size = 54 + 160 = 214
    let file_size = u32::from_le_bytes([output[2], output[3], output[4], output[5]]);
    assert_eq!(file_size, 214);

    // Data offset should be 54
    let data_offset = u32::from_le_bytes([output[10], output[11], output[12], output[13]]);
    assert_eq!(data_offset, 54);
}

#[test]
fn test_bmpwriter_invalid_bits() {
    let mut output = Vec::new();
    let mut cursor = Cursor::new(&mut output);
    let result = BmpWriter::new(&mut cursor, 16, 10, 5);
    assert!(result.is_err());
}

// =============================================================================
// BmpWriter Tests - Info Header
// =============================================================================

#[test]
fn test_bmpwriter_info_header() {
    let mut output = Vec::new();
    {
        let mut cursor = Cursor::new(&mut output);
        let _writer = BmpWriter::new(&mut cursor, 24, 100, 50).unwrap();
    }

    // Info header starts at offset 14
    // Info header size should be 40
    let info_size = u32::from_le_bytes([output[14], output[15], output[16], output[17]]);
    assert_eq!(info_size, 40);

    // Width at offset 18 (4 bytes, little endian)
    let width = i32::from_le_bytes([output[18], output[19], output[20], output[21]]);
    assert_eq!(width, 100);

    // Height at offset 22 (4 bytes, little endian)
    let height = i32::from_le_bytes([output[22], output[23], output[24], output[25]]);
    assert_eq!(height, 50);

    // Planes at offset 26 (2 bytes) should be 1
    let planes = u16::from_le_bytes([output[26], output[27]]);
    assert_eq!(planes, 1);

    // Bits per pixel at offset 28 (2 bytes) should be 24
    let bits = u16::from_le_bytes([output[28], output[29]]);
    assert_eq!(bits, 24);
}

// =============================================================================
// BmpWriter Tests - Color Table
// =============================================================================

#[test]
fn test_bmpwriter_1bit_color_table() {
    let mut output = Vec::new();
    {
        let mut cursor = Cursor::new(&mut output);
        let _writer = BmpWriter::new(&mut cursor, 1, 8, 4).unwrap();
    }

    // Color table starts at offset 54 (14 + 40)
    // 1-bit has 2 colors: black (0,0,0,0) and white (255,255,255,0)
    assert_eq!(&output[54..58], &[0, 0, 0, 0]); // Black
    assert_eq!(&output[58..62], &[255, 255, 255, 0]); // White
}

#[test]
fn test_bmpwriter_8bit_color_table() {
    let mut output = Vec::new();
    {
        let mut cursor = Cursor::new(&mut output);
        let _writer = BmpWriter::new(&mut cursor, 8, 8, 4).unwrap();
    }

    // Color table starts at offset 54 (14 + 40)
    // 8-bit has 256 grayscale colors
    for i in 0..256 {
        let offset = 54 + i * 4;
        assert_eq!(output[offset], i as u8); // B
        assert_eq!(output[offset + 1], i as u8); // G
        assert_eq!(output[offset + 2], i as u8); // R
        assert_eq!(output[offset + 3], 0); // Reserved
    }
}

// =============================================================================
// BmpWriter Tests - Line Writing
// =============================================================================

#[test]
fn test_bmpwriter_write_line() {
    let mut output = Vec::new();
    output.resize(214, 0); // Pre-allocate for 24-bit, 10x5 image
    {
        let mut cursor = Cursor::new(&mut output);
        let mut writer = BmpWriter::new(&mut cursor, 24, 10, 5).unwrap();

        // Write line 0 (bottom line in BMP)
        let line_data = vec![0xFF; 30]; // 10 pixels * 3 bytes
        writer.write_line(&mut cursor, 0, &line_data).unwrap();
    }

    // Line 0 should be at the end of the file (BMP stores bottom-up)
    // Data starts at offset 54, linesize is 32
    // Line 0 is at pos1 - (0 + 1) * 32 = 54 + 160 - 32 = 182
    // Check that line data is written at correct position
    let data_start = 54 + 160 - 32; // 182
    assert_eq!(&output[data_start..data_start + 30], &[0xFF; 30]);
}

#[test]
fn test_bmpwriter_write_multiple_lines() {
    let mut output = Vec::new();
    output.resize(214, 0); // Pre-allocate for 24-bit, 10x5 image
    {
        let mut cursor = Cursor::new(&mut output);
        let mut writer = BmpWriter::new(&mut cursor, 24, 10, 5).unwrap();

        // Write lines with different patterns
        for y in 0..5 {
            let line_data = vec![y as u8; 30];
            writer.write_line(&mut cursor, y, &line_data).unwrap();
        }
    }

    // Verify lines are stored bottom-up
    let data_offset = 54;
    let linesize = 32;

    // Line 4 (top) should be at the start of data
    assert_eq!(output[data_offset], 4);
    // Line 3 should be next
    assert_eq!(output[data_offset + linesize], 3);
    // Line 0 (bottom) should be at the end
    assert_eq!(output[data_offset + linesize * 4], 0);
}

// =============================================================================
// BmpWriter Tests - Full Image
// =============================================================================

#[test]
fn test_bmpwriter_1bit_complete_image() {
    let mut output = Vec::new();
    output.resize(78, 0); // 62 header + 16 data for 8x4 1-bit image
    {
        let mut cursor = Cursor::new(&mut output);
        let mut writer = BmpWriter::new(&mut cursor, 1, 8, 4).unwrap();

        // Write 4 lines of alternating patterns
        for y in 0..4 {
            let line_data = if y % 2 == 0 { vec![0xAA] } else { vec![0x55] };
            writer.write_line(&mut cursor, y, &line_data).unwrap();
        }
    }

    // Verify BMP structure
    assert_eq!(&output[0..2], b"BM");

    // Verify data offset
    let data_offset = u32::from_le_bytes([output[10], output[11], output[12], output[13]]) as usize;
    assert_eq!(data_offset, 62);

    // Verify alternating pattern (bottom-up storage)
    // linesize is 4 bytes (aligned)
    // Line 3 (top in image, first in storage): 0x55
    assert_eq!(output[data_offset], 0x55);
    // Line 2: 0xAA
    assert_eq!(output[data_offset + 4], 0xAA);
    // Line 1: 0x55
    assert_eq!(output[data_offset + 8], 0x55);
    // Line 0 (bottom in image, last in storage): 0xAA
    assert_eq!(output[data_offset + 12], 0xAA);
}

#[test]
fn test_bmpwriter_8bit_grayscale_image() {
    let width = 4;
    let height = 2;
    let linesize = align32(width) as usize;
    let header_size = 14 + 40 + 256 * 4; // 1078
    let data_size = linesize * height;
    let file_size = header_size + data_size;

    let mut output = Vec::new();
    output.resize(file_size, 0);
    {
        let mut cursor = Cursor::new(&mut output);
        let mut writer = BmpWriter::new(&mut cursor, 8, width as i32, height as i32).unwrap();

        // Write gradient lines
        writer
            .write_line(&mut cursor, 0, &[0, 64, 128, 192])
            .unwrap();
        writer
            .write_line(&mut cursor, 1, &[255, 191, 127, 63])
            .unwrap();
    }

    // Verify lines (bottom-up storage)
    // Line 1 should be first in data area
    assert_eq!(output[header_size], 255);
    assert_eq!(output[header_size + 1], 191);
    // Line 0 should be second
    assert_eq!(output[header_size + linesize], 0);
    assert_eq!(output[header_size + linesize + 1], 64);
}
