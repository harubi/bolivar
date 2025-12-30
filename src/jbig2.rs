//! JBIG2 segment reader/writer - port of pdfminer.six jbig2.py
//!
//! Reads and writes JBIG2 segments from/to byte streams.

use std::io::{Read, Seek, SeekFrom, Write};

use crate::error::Result;

// Segment header flag masks
const HEADER_FLAG_DEFERRED: u8 = 0b1000_0000;
const HEADER_FLAG_PAGE_ASSOC_LONG: u8 = 0b0100_0000;
const SEG_TYPE_MASK: u8 = 0b0011_1111;

// Retention flags
const REF_COUNT_SHORT_MASK: u8 = 0b1110_0000;
const REF_COUNT_LONG_MASK: u32 = 0x1FFF_FFFF;
const REF_COUNT_LONG: u8 = 7;

// Data length
const DATA_LEN_UNKNOWN: u32 = 0xFFFF_FFFF;

// Segment types
pub const SEG_TYPE_IMMEDIATE_GEN_REGION: u8 = 38;
pub const SEG_TYPE_END_OF_PAGE: u8 = 49;
pub const SEG_TYPE_END_OF_FILE: u8 = 51;

// File header
pub const FILE_HEADER_ID: &[u8] = b"\x97\x4a\x42\x32\x0d\x0a\x1a\x0a";
const FILE_HEAD_FLAG_SEQUENTIAL: u8 = 0b0000_0001;

/// Check if a specific bit is set
fn bit_set(bit_pos: u8, value: u8) -> bool {
    ((value >> bit_pos) & 1) != 0
}

/// Check if a flag is set using a mask
fn check_flag(flag: u8, value: u8) -> bool {
    (flag & value) != 0
}

/// Extract a masked value, shifting right to get actual value
fn masked_value(mask: u8, value: u8) -> u8 {
    for bit_pos in 0..8 {
        if bit_set(bit_pos, mask) {
            return (value & mask) >> bit_pos;
        }
    }
    0
}

/// Create a masked value by shifting left
fn mask_value(mask: u8, value: u8) -> u8 {
    for bit_pos in 0..8 {
        if bit_set(bit_pos, mask) {
            return (value & (mask >> bit_pos)) << bit_pos;
        }
    }
    0
}

/// Segment flags parsed from the flags byte
#[derive(Debug, Clone, Default)]
pub struct Jbig2SegmentFlags {
    pub deferred: bool,
    pub page_assoc_long: bool,
    pub seg_type: u8,
}

/// Retention flags containing reference information
#[derive(Debug, Clone, Default)]
pub struct Jbig2RetentionFlags {
    pub ref_count: u32,
    pub retain_segments: Vec<bool>,
    pub ref_segments: Vec<u32>,
}

/// A JBIG2 segment with header and data
#[derive(Debug, Clone)]
pub struct Jbig2Segment {
    pub number: u32,
    pub flags: Jbig2SegmentFlags,
    pub retention_flags: Jbig2RetentionFlags,
    pub page_assoc: u32,
    pub data_length: u32,
    pub raw_data: Vec<u8>,
}

impl Jbig2Segment {
    pub fn new(number: u32, seg_type: u8, page_assoc: u32, raw_data: Vec<u8>) -> Self {
        Self {
            number,
            flags: Jbig2SegmentFlags {
                deferred: false,
                page_assoc_long: page_assoc > 255,
                seg_type,
            },
            retention_flags: Jbig2RetentionFlags::default(),
            page_assoc,
            data_length: raw_data.len() as u32,
            raw_data,
        }
    }

    fn new_eop(seg_number: u32, page_number: u32) -> Self {
        Self {
            number: seg_number,
            flags: Jbig2SegmentFlags {
                deferred: false,
                page_assoc_long: page_number > 255,
                seg_type: SEG_TYPE_END_OF_PAGE,
            },
            retention_flags: Jbig2RetentionFlags::default(),
            page_assoc: page_number,
            data_length: 0,
            raw_data: Vec::new(),
        }
    }

    fn new_eof(seg_number: u32) -> Self {
        Self {
            number: seg_number,
            flags: Jbig2SegmentFlags {
                deferred: false,
                page_assoc_long: false,
                seg_type: SEG_TYPE_END_OF_FILE,
            },
            retention_flags: Jbig2RetentionFlags::default(),
            page_assoc: 0,
            data_length: 0,
            raw_data: Vec::new(),
        }
    }
}

/// Reads JBIG2 segments from a byte stream
pub struct Jbig2StreamReader<'a, R: Read + Seek> {
    stream: &'a mut R,
}

impl<'a, R: Read + Seek> Jbig2StreamReader<'a, R> {
    pub fn new(stream: &'a mut R) -> Self {
        Self { stream }
    }

    pub fn get_segments(&mut self) -> Result<Vec<Jbig2Segment>> {
        let mut segments = Vec::new();

        while !self.is_eof()? {
            match self.read_segment() {
                Ok(Some(segment)) => segments.push(segment),
                Ok(None) => break, // Truncated header
                Err(_) => break,
            }
        }

        Ok(segments)
    }

    fn is_eof(&mut self) -> Result<bool> {
        let mut buf = [0u8; 1];
        match self.stream.read(&mut buf) {
            Ok(0) => Ok(true),
            Ok(_) => {
                self.stream.seek(SeekFrom::Current(-1))?;
                Ok(false)
            }
            Err(_) => Ok(true),
        }
    }

    fn read_segment(&mut self) -> Result<Option<Jbig2Segment>> {
        // Read segment number (4 bytes)
        let mut num_buf = [0u8; 4];
        if self.stream.read(&mut num_buf)? < 4 {
            return Ok(None);
        }
        let number = u32::from_be_bytes(num_buf);

        // Read flags (1 byte)
        let mut flags_buf = [0u8; 1];
        if self.stream.read(&mut flags_buf)? < 1 {
            return Ok(None);
        }
        let flags_byte = flags_buf[0];
        let flags = self.parse_flags(flags_byte);

        // Read retention flags (variable length)
        let retention_flags = self.parse_retention_flags(number)?;

        // Read page association (1 or 4 bytes)
        let page_assoc = if flags.page_assoc_long {
            let mut buf = [0u8; 4];
            if self.stream.read(&mut buf)? < 4 {
                return Ok(None);
            }
            u32::from_be_bytes(buf)
        } else {
            let mut buf = [0u8; 1];
            if self.stream.read(&mut buf)? < 1 {
                return Ok(None);
            }
            buf[0] as u32
        };

        // Read data length (4 bytes)
        let mut len_buf = [0u8; 4];
        if self.stream.read(&mut len_buf)? < 4 {
            return Ok(None);
        }
        let data_length = u32::from_be_bytes(len_buf);

        // Read raw data
        let raw_data = if data_length > 0 && data_length != DATA_LEN_UNKNOWN {
            let mut buf = vec![0u8; data_length as usize];
            self.stream.read_exact(&mut buf)?;
            buf
        } else if data_length == DATA_LEN_UNKNOWN && flags.seg_type == SEG_TYPE_IMMEDIATE_GEN_REGION
        {
            return Err(crate::error::PdfError::DecodeError(
                "Unknown segment length not implemented".into(),
            ));
        } else {
            Vec::new()
        };

        Ok(Some(Jbig2Segment {
            number,
            flags,
            retention_flags,
            page_assoc,
            data_length,
            raw_data,
        }))
    }

    fn parse_flags(&self, flags: u8) -> Jbig2SegmentFlags {
        Jbig2SegmentFlags {
            deferred: check_flag(HEADER_FLAG_DEFERRED, flags),
            page_assoc_long: check_flag(HEADER_FLAG_PAGE_ASSOC_LONG, flags),
            seg_type: masked_value(SEG_TYPE_MASK, flags),
        }
    }

    fn parse_retention_flags(&mut self, seg_num: u32) -> Result<Jbig2RetentionFlags> {
        // Read first byte
        let mut first_byte = [0u8; 1];
        self.stream.read_exact(&mut first_byte)?;
        let flags = first_byte[0];

        let mut ref_count = masked_value(REF_COUNT_SHORT_MASK, flags) as u32;
        let mut retain_segments = Vec::new();

        if ref_count < REF_COUNT_LONG as u32 {
            // Short form: retain flags in lower 5 bits
            for bit_pos in 0..5 {
                retain_segments.push(bit_set(bit_pos, flags));
            }
        } else {
            // Long form: read 3 more bytes for 4-byte ref count
            let mut extra = [0u8; 3];
            self.stream.read_exact(&mut extra)?;
            let full_bytes = [first_byte[0], extra[0], extra[1], extra[2]];
            let full_value = u32::from_be_bytes(full_bytes);
            ref_count = full_value & REF_COUNT_LONG_MASK;

            // Read retention bytes
            let ret_bytes_count = (ref_count + 1).div_ceil(8);
            for _ in 0..ret_bytes_count {
                let mut ret_byte = [0u8; 1];
                self.stream.read_exact(&mut ret_byte)?;
                for bit_pos in 0..8 {
                    retain_segments.push(bit_set(bit_pos, ret_byte[0]));
                }
            }
        }

        // Determine reference segment size based on segment number
        let ref_size = if seg_num <= 256 {
            1
        } else if seg_num <= 65536 {
            2
        } else {
            4
        };

        // Read referenced segments
        let mut ref_segments = Vec::new();
        for _ in 0..ref_count {
            let ref_val = match ref_size {
                1 => {
                    let mut buf = [0u8; 1];
                    self.stream.read_exact(&mut buf)?;
                    buf[0] as u32
                }
                2 => {
                    let mut buf = [0u8; 2];
                    self.stream.read_exact(&mut buf)?;
                    u16::from_be_bytes(buf) as u32
                }
                _ => {
                    let mut buf = [0u8; 4];
                    self.stream.read_exact(&mut buf)?;
                    u32::from_be_bytes(buf)
                }
            };
            ref_segments.push(ref_val);
        }

        Ok(Jbig2RetentionFlags {
            ref_count,
            retain_segments,
            ref_segments,
        })
    }
}

/// Writes JBIG2 segments to a byte stream
pub struct Jbig2StreamWriter<'a, W: Write> {
    stream: &'a mut W,
}

impl<'a, W: Write> Jbig2StreamWriter<'a, W> {
    pub fn new(stream: &'a mut W) -> Self {
        Self { stream }
    }

    pub fn write_segments(
        &mut self,
        segments: &[Jbig2Segment],
        fix_last_page: bool,
    ) -> Result<usize> {
        let mut data_len = 0;
        let mut current_page: Option<u32> = None;
        let mut last_seg_num: Option<u32> = None;

        for segment in segments {
            let data = self.encode_segment(segment);
            self.stream.write_all(&data)?;
            data_len += data.len();

            last_seg_num = Some(segment.number);

            if fix_last_page {
                if segment.flags.seg_type == SEG_TYPE_END_OF_PAGE {
                    current_page = None;
                } else if segment.page_assoc > 0 {
                    current_page = Some(segment.page_assoc);
                }
            }
        }

        // Add end-of-page if needed
        if fix_last_page {
            if let (Some(page), Some(seg_num)) = (current_page, last_seg_num) {
                let eop = Jbig2Segment::new_eop(seg_num + 1, page);
                let data = self.encode_segment(&eop);
                self.stream.write_all(&data)?;
                data_len += data.len();
            }
        }

        Ok(data_len)
    }

    pub fn write_file(&mut self, segments: &[Jbig2Segment], fix_last_page: bool) -> Result<usize> {
        // Write file header
        let mut header = Vec::new();
        header.extend_from_slice(FILE_HEADER_ID);
        header.push(FILE_HEAD_FLAG_SEQUENTIAL);
        header.extend_from_slice(&1u32.to_be_bytes()); // number of pages = 1

        self.stream.write_all(&header)?;
        let mut data_len = header.len();

        // Write segments
        data_len += self.write_segments(segments, fix_last_page)?;

        // Calculate EOF segment number
        let mut seg_num = 0u32;
        for segment in segments {
            seg_num = segment.number;
        }

        let seg_num_offset = if fix_last_page { 2 } else { 1 };
        let eof = Jbig2Segment::new_eof(seg_num + seg_num_offset);
        let data = self.encode_segment(&eof);
        self.stream.write_all(&data)?;
        data_len += data.len();

        Ok(data_len)
    }

    fn encode_segment(&self, segment: &Jbig2Segment) -> Vec<u8> {
        let mut data = Vec::new();

        // Segment number (4 bytes)
        data.extend_from_slice(&segment.number.to_be_bytes());

        // Flags (1 byte)
        data.extend_from_slice(&self.encode_flags(&segment.flags, segment));

        // Retention flags (variable)
        data.extend_from_slice(&self.encode_retention_flags(&segment.retention_flags, segment));

        // Page association (1 or 4 bytes)
        if segment.flags.page_assoc_long || segment.page_assoc > 255 {
            data.extend_from_slice(&segment.page_assoc.to_be_bytes());
        } else {
            data.push(segment.page_assoc as u8);
        }

        // Data length (4 bytes) + raw data
        data.extend_from_slice(&segment.data_length.to_be_bytes());
        data.extend_from_slice(&segment.raw_data);

        data
    }

    fn encode_flags(&self, flags: &Jbig2SegmentFlags, segment: &Jbig2Segment) -> Vec<u8> {
        let mut flag_byte = 0u8;

        if flags.deferred {
            flag_byte |= HEADER_FLAG_DEFERRED;
        }

        if flags.page_assoc_long || segment.page_assoc > 255 {
            flag_byte |= HEADER_FLAG_PAGE_ASSOC_LONG;
        }

        flag_byte |= mask_value(SEG_TYPE_MASK, flags.seg_type);

        vec![flag_byte]
    }

    fn encode_retention_flags(
        &self,
        retention: &Jbig2RetentionFlags,
        segment: &Jbig2Segment,
    ) -> Vec<u8> {
        let mut data = Vec::new();
        let ref_count = retention.ref_count;

        if ref_count <= 4 {
            // Short form
            let mut flags_byte = mask_value(REF_COUNT_SHORT_MASK, ref_count as u8);
            for (idx, &retain) in retention.retain_segments.iter().take(5).enumerate() {
                if retain {
                    flags_byte |= 1 << idx;
                }
            }
            data.push(flags_byte);
        } else {
            // Long form
            let bytes_count = (ref_count + 1).div_ceil(8);
            let flags_dword =
                ((mask_value(REF_COUNT_SHORT_MASK, REF_COUNT_LONG) as u32) << 24) | ref_count;
            data.extend_from_slice(&flags_dword.to_be_bytes());

            for byte_idx in 0..bytes_count as usize {
                let mut ret_byte = 0u8;
                for bit_pos in 0..8 {
                    let seg_idx = byte_idx * 8 + bit_pos;
                    if seg_idx < retention.retain_segments.len()
                        && retention.retain_segments[seg_idx]
                    {
                        ret_byte |= 1 << bit_pos;
                    }
                }
                data.push(ret_byte);
            }
        }

        // Reference segments
        let seg_num = segment.number;
        for &ref_seg in &retention.ref_segments {
            if seg_num <= 256 {
                data.push(ref_seg as u8);
            } else if seg_num <= 65536 {
                data.extend_from_slice(&(ref_seg as u16).to_be_bytes());
            } else {
                data.extend_from_slice(&ref_seg.to_be_bytes());
            }
        }

        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bit_set() {
        assert!(bit_set(0, 0b0000_0001));
        assert!(bit_set(7, 0b1000_0000));
        assert!(!bit_set(0, 0b0000_0010));
    }

    #[test]
    fn test_masked_value() {
        assert_eq!(masked_value(SEG_TYPE_MASK, 38), 38);
        assert_eq!(masked_value(REF_COUNT_SHORT_MASK, 0b1010_0000), 5);
    }

    #[test]
    fn test_mask_value() {
        assert_eq!(mask_value(SEG_TYPE_MASK, 38), 38);
        assert_eq!(mask_value(REF_COUNT_SHORT_MASK, 5), 0b1010_0000);
    }
}
