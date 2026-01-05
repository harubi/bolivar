//! Image export functionality - BMP writing and image format handling.
//!
//! Port of pdfminer.six image.py

use crate::codec::ascii85::{ascii85decode, asciihexdecode};
use crate::codec::lzw::lzwdecode_with_earlychange;
use crate::codec::runlength::rldecode;
use crate::pdftypes::{PDFObject, PDFStream};
use crate::{PdfError, Result};
use flate2::read::ZlibDecoder;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const MAX_IMAGE_DECODED_BYTES: usize = 256 * 1024 * 1024;

/// Align a value to a 4-byte boundary (32-bit alignment for BMP rows).
pub const fn align32(x: i32) -> i32 {
    ((x + 3) / 4) * 4
}

/// BMP file writer for creating bitmap images.
///
/// Supports 1-bit (B&W), 8-bit (grayscale), and 24-bit (RGB) images.
/// BMP stores rows bottom-up, so line 0 is at the bottom of the image.
pub struct BmpWriter {
    linesize: i32,
    #[allow(dead_code)]
    pos0: u64,
    pos1: u64,
}

/// Image writer for exporting PDF images to files.
pub struct ImageWriter {
    outdir: PathBuf,
    seq: usize,
}

impl ImageWriter {
    pub fn new(outdir: impl AsRef<Path>) -> Result<Self> {
        let outdir = outdir.as_ref().to_path_buf();
        fs::create_dir_all(&outdir)?;
        Ok(Self { outdir, seq: 0 })
    }

    fn next_path(&mut self, name: &str, ext: &str) -> PathBuf {
        let base = name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>();
        let base = if base.is_empty() {
            "image".to_string()
        } else {
            base
        };
        self.seq += 1;
        let filename = format!("{}_{}{}", base, self.seq, ext);
        self.outdir.join(filename)
    }

    /// Write data to a file and return the filename.
    fn write_and_return_filename(&mut self, name: &str, ext: &str, data: &[u8]) -> Result<String> {
        let path = self.next_path(name, ext);
        fs::write(&path, data)?;
        Ok(path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string())
    }

    /// Export a PDF image stream to disk.
    pub fn export_image(
        &mut self,
        name: &str,
        stream: &PDFStream,
        srcsize: (Option<i32>, Option<i32>),
        bits: i32,
        colorspace: &[String],
    ) -> Result<String> {
        let filters = get_filters(stream);
        let last_filter = filters.last().map(|(f, _)| f.as_str());
        let mut too_large = false;
        let mut max_len = Some(MAX_IMAGE_DECODED_BYTES);
        if let (Some(w), Some(h)) = srcsize
            && let Some(len) = expected_image_len_uncapped(w, h, bits, colorspace)
        {
            if len > MAX_IMAGE_DECODED_BYTES {
                too_large = true;
            }
            let extra = predictor_overhead(&filters, h);
            max_len = Some(len.saturating_add(extra).min(MAX_IMAGE_DECODED_BYTES));
        }

        let is_encoded_image =
            last_filter.is_some_and(|f| is_dct_decode(f) || is_jpx_decode(f) || is_jbig2_decode(f));

        if too_large && !is_encoded_image {
            return self.write_and_return_filename(name, ".bin", stream.get_rawdata());
        }

        let data = decode_stream_data_limited(stream, &filters, max_len)?;

        if last_filter.is_some_and(is_dct_decode) {
            return self.write_and_return_filename(name, ".jpg", &data);
        }

        if last_filter.is_some_and(is_jpx_decode) {
            return self.write_and_return_filename(name, ".jp2", &data);
        }

        if last_filter.is_some_and(is_jbig2_decode) {
            return self.write_and_return_filename(name, ".jb2", &data);
        }

        // Default to BMP for 8-bit grayscale/RGB images
        let (width, height) = match srcsize {
            (Some(w), Some(h)) => (w, h),
            _ => return self.write_and_return_filename(name, ".bin", &data),
        };

        if width <= 0 || height <= 0 {
            return self.write_and_return_filename(name, ".bin", &data);
        }

        if bits == 8 && colorspace.iter().any(|c| is_device_gray_name(c)) {
            return save_bmp(
                &mut self.seq,
                &self.outdir,
                name,
                8,
                width,
                height,
                1,
                &data,
            );
        }

        if bits == 8 && colorspace.iter().any(|c| is_device_rgb_name(c)) {
            return save_bmp(
                &mut self.seq,
                &self.outdir,
                name,
                24,
                width,
                height,
                3,
                &data,
            );
        }

        if bits == 1 {
            return save_bmp(
                &mut self.seq,
                &self.outdir,
                name,
                1,
                width,
                height,
                1,
                &data,
            );
        }

        // Fallback: write raw bytes
        self.write_and_return_filename(name, ".bin", &data)
    }
}

const fn is_dct_decode(name: &str) -> bool {
    name.eq_ignore_ascii_case("DCTDecode") || name.eq_ignore_ascii_case("DCT")
}

const fn is_jpx_decode(name: &str) -> bool {
    name.eq_ignore_ascii_case("JPXDecode") || name.eq_ignore_ascii_case("JPX")
}

const fn is_jbig2_decode(name: &str) -> bool {
    name.eq_ignore_ascii_case("JBIG2Decode")
}

/// Check if name matches a colorspace (full name or inline abbreviation).
fn matches_colorspace(name: &str, full_name: &str, abbreviation: &str) -> bool {
    name == full_name || name.eq_ignore_ascii_case(abbreviation)
}

fn is_device_gray_name(name: &str) -> bool {
    matches_colorspace(name, "DeviceGray", "G")
}

fn is_device_rgb_name(name: &str) -> bool {
    matches_colorspace(name, "DeviceRGB", "RGB")
}

fn is_device_cmyk_name(name: &str) -> bool {
    matches_colorspace(name, "DeviceCMYK", "CMYK")
}

fn expected_image_len_uncapped(
    width: i32,
    height: i32,
    bits: i32,
    colorspace: &[String],
) -> Option<usize> {
    if width <= 0 || height <= 0 || bits <= 0 {
        return None;
    }
    let w = width as usize;
    let h = height as usize;
    if bits == 1 {
        let row = w.div_ceil(8);
        return row.checked_mul(h);
    }
    if bits != 8 {
        return None;
    }
    let channels = if colorspace.iter().any(|c| is_device_cmyk_name(c)) {
        4
    } else if colorspace.iter().any(|c| is_device_rgb_name(c)) {
        3
    } else {
        1 // Default for Indexed, DeviceGray, or unknown
    };
    w.checked_mul(h)?.checked_mul(channels)
}

fn predictor_overhead(
    filters: &[(String, Option<HashMap<String, PDFObject>>)],
    height: i32,
) -> usize {
    if height <= 0 {
        return 0;
    }
    for (_, params) in filters {
        if let Some(p) = params {
            let pred = p
                .get("Predictor")
                .and_then(|v| v.as_int().ok())
                .unwrap_or(1);
            // PDF Predictor values:
            // 2 = TIFF Predictor 2 (horizontal differencing)
            // 10-15 = PNG predictors (each row has 1-byte filter prefix)
            if pred == 2 || pred >= 10 {
                return height as usize;
            }
        }
    }
    0
}

#[allow(dead_code)]
fn expected_image_len(width: i32, height: i32, bits: i32, colorspace: &[String]) -> Option<usize> {
    expected_image_len_uncapped(width, height, bits, colorspace)
        .map(|len| len.min(MAX_IMAGE_DECODED_BYTES))
}

#[cfg(test)]
mod tests {
    use super::{MAX_IMAGE_DECODED_BYTES, expected_image_len};

    #[test]
    fn expected_image_len_caps_large_images() {
        let colorspace = vec!["DeviceRGB".to_string()];
        let len = expected_image_len(10_000, 10_000, 8, &colorspace).unwrap();
        assert_eq!(len, MAX_IMAGE_DECODED_BYTES);
    }

    #[test]
    fn expected_image_len_small_images_unchanged() {
        let colorspace = vec!["DeviceRGB".to_string()];
        let len = expected_image_len(100, 100, 8, &colorspace).unwrap();
        assert_eq!(len, 100 * 100 * 3);
    }
}

fn get_filters(stream: &PDFStream) -> Vec<(String, Option<HashMap<String, PDFObject>>)> {
    let filter_obj = stream.get("Filter");
    let params_obj = stream.get("DecodeParms");

    let filters: Vec<PDFObject> = match filter_obj {
        Some(PDFObject::Name(_)) => vec![filter_obj.unwrap().clone()],
        Some(PDFObject::Array(arr)) => arr.clone(),
        _ => Vec::new(),
    };

    let params_list: Vec<Option<HashMap<String, PDFObject>>> = match params_obj {
        Some(PDFObject::Dict(d)) => vec![Some(d.clone())],
        Some(PDFObject::Array(arr)) => arr
            .iter()
            .map(|obj| match obj {
                PDFObject::Dict(d) => Some(d.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };

    let mut result = Vec::new();
    if filters.is_empty() {
        return result;
    }

    let params = if params_list.is_empty() {
        vec![None; filters.len()]
    } else if params_list.len() == 1 && filters.len() > 1 {
        vec![params_list[0].clone(); filters.len()]
    } else {
        params_list
    };

    for (idx, filter) in filters.into_iter().enumerate() {
        if let PDFObject::Name(name) = filter {
            let p = params.get(idx).cloned().unwrap_or(None);
            result.push((name, p));
        }
    }

    result
}

fn decode_stream_data_limited(
    stream: &PDFStream,
    filters: &[(String, Option<HashMap<String, PDFObject>>)],
    max_len: Option<usize>,
) -> Result<Vec<u8>> {
    let mut data = stream.get_rawdata().to_vec();

    for (filter, params) in filters {
        if filter.eq_ignore_ascii_case("FlateDecode") || filter.eq_ignore_ascii_case("Fl") {
            data = flate_decode_limited(&data, max_len)?;
        } else if filter.eq_ignore_ascii_case("LZWDecode") || filter.eq_ignore_ascii_case("LZW") {
            let early_change = params
                .as_ref()
                .and_then(|p| p.get("EarlyChange"))
                .and_then(|v| v.as_int().ok())
                .unwrap_or(1) as i32;
            data = lzwdecode_with_earlychange(&data, early_change)?;
            enforce_max_len(data.len(), max_len)?;
        } else if filter.eq_ignore_ascii_case("ASCII85Decode") || filter.eq_ignore_ascii_case("A85")
        {
            data = ascii85decode(&data)?;
            enforce_max_len(data.len(), max_len)?;
        } else if filter.eq_ignore_ascii_case("ASCIIHexDecode")
            || filter.eq_ignore_ascii_case("AHx")
        {
            data = asciihexdecode(&data)?;
            enforce_max_len(data.len(), max_len)?;
        } else if filter.eq_ignore_ascii_case("RunLengthDecode")
            || filter.eq_ignore_ascii_case("RL")
        {
            data = rldecode(&data)?;
            enforce_max_len(data.len(), max_len)?;
        } else if is_dct_decode(filter) || is_jpx_decode(filter) || is_jbig2_decode(filter) {
            // Leave data as-is for image formats
            break;
        }

        if let Some(p) = params {
            let predictor = p
                .get("Predictor")
                .and_then(|v| v.as_int().ok())
                .unwrap_or(1);
            if predictor == 2 {
                let colors = p.get("Colors").and_then(|v| v.as_int().ok()).unwrap_or(1) as usize;
                let columns = p.get("Columns").and_then(|v| v.as_int().ok()).unwrap_or(1) as usize;
                let bits = p
                    .get("BitsPerComponent")
                    .and_then(|v| v.as_int().ok())
                    .unwrap_or(8) as usize;
                data = apply_tiff_predictor(colors, columns, bits, &data)?;
                enforce_max_len(data.len(), max_len)?;
            } else if predictor >= 10 {
                let colors = p.get("Colors").and_then(|v| v.as_int().ok()).unwrap_or(1) as usize;
                let columns = p.get("Columns").and_then(|v| v.as_int().ok()).unwrap_or(1) as usize;
                let bits = p
                    .get("BitsPerComponent")
                    .and_then(|v| v.as_int().ok())
                    .unwrap_or(8) as usize;
                data = apply_png_predictor(&data, columns, colors, bits)?;
                enforce_max_len(data.len(), max_len)?;
            }
        }
    }

    Ok(data)
}

fn enforce_max_len(len: usize, max_len: Option<usize>) -> Result<()> {
    if let Some(max) = max_len
        && len > max
    {
        return Err(PdfError::DecodeError(format!(
            "decoded data exceeds expected size ({} > {})",
            len, max
        )));
    }
    Ok(())
}

fn flate_decode_limited(data: &[u8], max_len: Option<usize>) -> Result<Vec<u8>> {
    let mut decoder = ZlibDecoder::new(data);
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = decoder
            .read(&mut buf)
            .map_err(|e| PdfError::DecodeError(format!("FlateDecode error: {}", e)))?;
        if n == 0 {
            break;
        }
        if let Some(max) = max_len
            && out.len().saturating_add(n) > max
        {
            return Err(PdfError::DecodeError(format!(
                "decoded data exceeds expected size ({} > {})",
                out.len() + n,
                max
            )));
        }
        out.extend_from_slice(&buf[..n]);
    }
    Ok(out)
}

fn apply_tiff_predictor(
    colors: usize,
    columns: usize,
    bits_per_component: usize,
    data: &[u8],
) -> Result<Vec<u8>> {
    if bits_per_component != 8 {
        return Ok(data.to_vec());
    }
    let bpp = colors * (bits_per_component / 8);
    let nbytes = columns * bpp;
    let mut out = Vec::with_capacity(data.len());
    for row in data.chunks(nbytes) {
        let mut raw = Vec::with_capacity(nbytes);
        for i in 0..row.len() {
            let mut v = row[i];
            if i >= bpp {
                v = v.wrapping_add(raw[i - bpp]);
            }
            raw.push(v);
        }
        out.extend_from_slice(&raw);
    }
    Ok(out)
}

fn apply_png_predictor(
    data: &[u8],
    columns: usize,
    colors: usize,
    bits_per_component: usize,
) -> Result<Vec<u8>> {
    let row_bytes = colors * columns * bits_per_component / 8;
    let bpp = std::cmp::max(1, colors * bits_per_component / 8);
    let row_size = row_bytes + 1;

    let mut result = Vec::with_capacity(data.len());
    let mut prev_row = vec![0u8; row_bytes];

    for row_start in (0..data.len()).step_by(row_size) {
        if row_start + row_size > data.len() {
            break;
        }

        let filter_type = data[row_start];
        let row_data = &data[row_start + 1..row_start + row_size];
        let mut current_row = vec![0u8; row_bytes];

        match filter_type {
            0 => current_row.copy_from_slice(row_data),
            1 => {
                for i in 0..row_bytes {
                    let left = if i >= bpp { current_row[i - bpp] } else { 0 };
                    current_row[i] = row_data[i].wrapping_add(left);
                }
            }
            2 => {
                for i in 0..row_bytes {
                    current_row[i] = row_data[i].wrapping_add(prev_row[i]);
                }
            }
            3 => {
                for i in 0..row_bytes {
                    let left = if i >= bpp {
                        current_row[i - bpp] as u16
                    } else {
                        0
                    };
                    let above = prev_row[i] as u16;
                    current_row[i] = row_data[i].wrapping_add(((left + above) / 2) as u8);
                }
            }
            4 => {
                for i in 0..row_bytes {
                    let left = if i >= bpp { current_row[i - bpp] } else { 0 };
                    let above = prev_row[i];
                    let upper_left = if i >= bpp { prev_row[i - bpp] } else { 0 };
                    let paeth = paeth_predictor(left, above, upper_left);
                    current_row[i] = row_data[i].wrapping_add(paeth);
                }
            }
            _ => return Err(PdfError::DecodeError("invalid PNG predictor".to_string())),
        }

        result.extend_from_slice(&current_row);
        prev_row = current_row;
    }

    Ok(result)
}

const fn paeth_predictor(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i32 + b as i32 - c as i32;
    let pa = (p - a as i32).abs();
    let pb = (p - b as i32).abs();
    let pc = (p - c as i32).abs();
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

fn save_bmp(
    seq: &mut usize,
    outdir: &Path,
    name: &str,
    bits: i32,
    width: i32,
    height: i32,
    bytes_per_pixel: i32,
    data: &[u8],
) -> Result<String> {
    let base = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>();
    let base = if base.is_empty() {
        "image".to_string()
    } else {
        base
    };
    *seq += 1;
    let filename = format!("{}_{}.bmp", base, *seq);
    let path = outdir.join(&filename);

    let mut fp = File::create(&path)?;
    let mut writer = BmpWriter::new(&mut fp, bits, width, height)?;

    let row_bytes = if bits == 1 {
        ((width + 7) / 8) as usize
    } else {
        (width * bytes_per_pixel) as usize
    };
    let line_size = align32(row_bytes as i32) as usize;

    for y in 0..height {
        let start = (y as usize) * row_bytes;
        let end = start + row_bytes;
        let mut line = vec![0u8; line_size];
        if start < data.len() {
            let copy_end = end.min(data.len());
            line[..(copy_end - start)].copy_from_slice(&data[start..copy_end]);
        }
        writer.write_line(&mut fp, y, &line)?;
    }

    Ok(filename)
}

impl BmpWriter {
    /// Create a new BMP writer and write the header.
    ///
    /// # Arguments
    /// * `fp` - Output stream implementing Write + Seek
    /// * `bits` - Bits per pixel (1, 8, or 24)
    /// * `width` - Image width in pixels
    /// * `height` - Image height in pixels
    ///
    /// # Errors
    /// Returns error if bits is not 1, 8, or 24.
    pub fn new<W: Write + Seek>(fp: &mut W, bits: i32, width: i32, height: i32) -> Result<Self> {
        let ncols = match bits {
            1 => 2,
            8 => 256,
            24 => 0,
            _ => {
                return Err(PdfError::DecodeError(format!(
                    "Invalid bits per pixel: {}",
                    bits
                )));
            }
        };

        let linesize = align32((width * bits + 7) / 8);
        let datasize = linesize * height;
        let headersize = 14 + 40 + ncols * 4;

        // BITMAPINFOHEADER (40 bytes)
        let mut info = Vec::with_capacity(40);
        info.extend_from_slice(&40u32.to_le_bytes()); // biSize
        info.extend_from_slice(&width.to_le_bytes()); // biWidth
        info.extend_from_slice(&height.to_le_bytes()); // biHeight
        info.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
        info.extend_from_slice(&(bits as u16).to_le_bytes()); // biBitCount
        info.extend_from_slice(&0u32.to_le_bytes()); // biCompression
        info.extend_from_slice(&(datasize as u32).to_le_bytes()); // biSizeImage
        info.extend_from_slice(&0i32.to_le_bytes()); // biXPelsPerMeter
        info.extend_from_slice(&0i32.to_le_bytes()); // biYPelsPerMeter
        info.extend_from_slice(&(ncols as u32).to_le_bytes()); // biClrUsed
        info.extend_from_slice(&0u32.to_le_bytes()); // biClrImportant
        debug_assert_eq!(info.len(), 40);

        // BITMAPFILEHEADER (14 bytes)
        let mut header = Vec::with_capacity(14);
        header.push(b'B');
        header.push(b'M');
        header.extend_from_slice(&((headersize + datasize) as u32).to_le_bytes()); // bfSize
        header.extend_from_slice(&0u16.to_le_bytes()); // bfReserved1
        header.extend_from_slice(&0u16.to_le_bytes()); // bfReserved2
        header.extend_from_slice(&(headersize as u32).to_le_bytes()); // bfOffBits
        debug_assert_eq!(header.len(), 14);

        fp.write_all(&header)?;
        fp.write_all(&info)?;

        // Write color table
        if ncols == 2 {
            // B&W color table
            for &i in &[0u8, 255u8] {
                fp.write_all(&[i, i, i, 0])?;
            }
        } else if ncols == 256 {
            // Grayscale color table
            for i in 0..=255u8 {
                fp.write_all(&[i, i, i, 0])?;
            }
        }

        let pos0 = fp.stream_position()?;
        let pos1 = pos0 + datasize as u64;

        Ok(Self {
            linesize,
            pos0,
            pos1,
        })
    }

    /// Write a scanline to the BMP file.
    ///
    /// # Arguments
    /// * `y` - Line number (0 = bottom of image in BMP coordinate system)
    /// * `data` - Raw pixel data for the line
    pub fn write_line<W: Write + Seek>(&mut self, fp: &mut W, y: i32, data: &[u8]) -> Result<()> {
        let seek_pos = self.pos1 - ((y + 1) as u64 * self.linesize as u64);
        fp.seek(SeekFrom::Start(seek_pos))?;
        fp.write_all(data)?;
        Ok(())
    }
}
