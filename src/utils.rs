//! Miscellaneous Routines - port of pdfminer.six utils.py
//!
//! Provides utility types and functions for PDF processing including:
//! - Geometric types (Point, Rect, Matrix)
//! - Matrix transformation operations
//! - Plane spatial index structure for efficient object lookup
//! - Text formatting functions (Roman numerals, alphabetic)
//! - Binary data helpers

use std::borrow::Cow;
use std::collections::HashSet;
use std::hash::Hash;

use rustc_hash::{FxHashMap, FxHashSet};

/// Maximum integer value for PDF compatibility (32-bit signed max).
pub const INF: i32 = i32::MAX;

/// Floating-point infinity for bounding box calculations.
pub const INF_F64: f64 = f64::MAX;

/// Small epsilon for floating-point comparisons.
pub const EPSILON: f64 = 1e-9;

/// A 2D point (x, y).
pub type Point = (f64, f64);

/// A rectangle defined by (x0, y0, x1, y1) where (x0, y0) is typically bottom-left
/// and (x1, y1) is top-right.
pub type Rect = (f64, f64, f64, f64);

/// A 6-element affine transformation matrix (a, b, c, d, e, f).
/// Transforms point (x, y) to (ax + cy + e, bx + dy + f).
pub type Matrix = (f64, f64, f64, f64, f64, f64);

/// Identity transformation matrix.
pub const MATRIX_IDENTITY: Matrix = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0);

/// Compares two floats for approximate equality.
#[inline]
pub fn approx_eq(a: f64, b: f64, epsilon: f64) -> bool {
    (a - b).abs() < epsilon
}

/// Multiplies two matrices: result = m1 * m0.
/// This applies m0 first, then m1.
pub fn mult_matrix(m1: Matrix, m0: Matrix) -> Matrix {
    let (a1, b1, c1, d1, e1, f1) = m1;
    let (a0, b0, c0, d0, e0, f0) = m0;
    (
        a0 * a1 + c0 * b1,
        b0 * a1 + d0 * b1,
        a0 * c1 + c0 * d1,
        b0 * c1 + d0 * d1,
        a0 * e1 + c0 * f1 + e0,
        b0 * e1 + d0 * f1 + f0,
    )
}

/// Translates a matrix by (x, y) inside the projection.
///
/// The matrix is changed so that its origin is at the specified point in its own
/// coordinate system. Note that this is different from translating it within the
/// original coordinate system.
pub fn translate_matrix(m: Matrix, v: Point) -> Matrix {
    let (a, b, c, d, e, f) = m;
    let (x, y) = v;
    (a, b, c, d, x * a + y * c + e, x * b + y * d + f)
}

/// Applies a matrix to a point.
pub fn apply_matrix_pt(m: Matrix, v: Point) -> Point {
    let (a, b, c, d, e, f) = m;
    let (x, y) = v;
    (a * x + c * y + e, b * x + d * y + f)
}

/// Applies a matrix to a rectangle.
///
/// Note that the result is not a rotated rectangle, but a rectangle with the same
/// orientation that tightly fits the outside of the rotated content.
pub fn apply_matrix_rect(m: Matrix, rect: Rect) -> Rect {
    let (x0, y0, x1, y1) = rect;
    let left_bottom = (x0, y0);
    let right_bottom = (x1, y0);
    let right_top = (x1, y1);
    let left_top = (x0, y1);

    let (left1, bottom1) = apply_matrix_pt(m, left_bottom);
    let (right1, bottom2) = apply_matrix_pt(m, right_bottom);
    let (right2, top1) = apply_matrix_pt(m, right_top);
    let (left2, top2) = apply_matrix_pt(m, left_top);

    (
        left1.min(left2).min(right1).min(right2),
        bottom1.min(bottom2).min(top1).min(top2),
        left1.max(left2).max(right1).max(right2),
        bottom1.max(bottom2).max(top1).max(top2),
    )
}

/// Equivalent to apply_matrix_pt(m, (p, q)) - apply_matrix_pt(m, (0, 0)).
/// Applies matrix transformation to a vector (ignoring translation).
pub fn apply_matrix_norm(m: Matrix, v: Point) -> Point {
    let (a, b, c, d, _e, _f) = m;
    let (p, q) = v;
    (a * p + c * q, b * p + d * q)
}

/// Returns a discrete range for spatial indexing.
fn drange(v0: f64, v1: f64, d: i32) -> std::ops::Range<i32> {
    let start = (v0 as i32) / d;
    let end = ((v1 as i32) + d) / d;
    start..end
}

/// Trait for objects that have a bounding box.
pub trait HasBBox {
    fn x0(&self) -> f64;
    fn y0(&self) -> f64;
    fn x1(&self) -> f64;
    fn y1(&self) -> f64;

    fn bbox(&self) -> Rect {
        (self.x0(), self.y0(), self.x1(), self.y1())
    }

    fn width(&self) -> f64 {
        self.x1() - self.x0()
    }

    fn height(&self) -> f64 {
        self.y1() - self.y0()
    }
}

/// A set-like data structure for objects placed on a plane.
///
/// Can efficiently find objects in a certain rectangular area using a grid-based
/// spatial index.
pub struct Plane<T> {
    seq: Vec<T>,
    objs: FxHashSet<usize>,
    grid: FxHashMap<(i32, i32), Vec<usize>>,
    gridsize: i32,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

impl<T: HasBBox + Eq + Hash> Plane<T> {
    /// Creates a new Plane with the given bounding box and grid size.
    pub fn new(bbox: Rect, gridsize: i32) -> Self {
        let (x0, y0, x1, y1) = bbox;
        Self {
            seq: Vec::new(),
            objs: FxHashSet::default(),
            grid: FxHashMap::default(),
            gridsize,
            x0,
            y0,
            x1,
            y1,
        }
    }

    /// Returns an iterator over grid cells that intersect the given bbox.
    fn get_range(&self, bbox: Rect) -> impl Iterator<Item = (i32, i32)> {
        let (x0, y0, x1, y1) = bbox;

        // Check if bbox is completely outside the plane
        if x1 <= self.x0 || self.x1 <= x0 || y1 <= self.y0 || self.y1 <= y0 {
            return itertools::Either::Left(std::iter::empty());
        }

        let x0 = x0.max(self.x0);
        let y0 = y0.max(self.y0);
        let x1 = x1.min(self.x1);
        let y1 = y1.min(self.y1);

        let y_range = drange(y0, y1, self.gridsize);
        let x_range = drange(x0, x1, self.gridsize);

        itertools::Either::Right(
            y_range.flat_map(move |gy| x_range.clone().map(move |gx| (gx, gy))),
        )
    }

    /// Adds an object to the plane.
    pub fn add(&mut self, obj: T) {
        let idx = self.seq.len();
        let bbox = obj.bbox();

        let keys: Vec<_> = self.get_range(bbox).collect();
        for k in keys {
            self.grid
                .entry(k)
                .or_insert_with(|| Vec::with_capacity(8))
                .push(idx);
        }

        self.seq.push(obj);
        self.objs.insert(idx);
    }

    /// Adds multiple objects to the plane.
    pub fn extend(&mut self, objs: impl IntoIterator<Item = T>) {
        for obj in objs {
            self.add(obj);
        }
    }

    /// Removes an object from the plane.
    pub fn remove(&mut self, obj: &T) -> bool
    where
        T: PartialEq,
    {
        let idx = self.seq.iter().position(|o| o == obj);
        if let Some(idx) = idx {
            if self.objs.remove(&idx) {
                let bbox = self.seq[idx].bbox();
                let keys: Vec<_> = self.get_range(bbox).collect();
                for k in keys {
                    if let Some(v) = self.grid.get_mut(&k) {
                        v.retain(|&i| i != idx);
                    }
                }
                return true;
            }
        }
        false
    }

    /// Finds objects that intersect the given bounding box.
    pub fn find(&self, bbox: Rect) -> Vec<&T> {
        self.find_with_indices(bbox)
            .into_iter()
            .map(|(_, obj)| obj)
            .collect()
    }

    /// Finds objects that intersect the given bounding box, returning (index, object) pairs.
    /// The index is the position in the internal seq (insertion order).
    pub fn find_with_indices(&self, bbox: Rect) -> Vec<(usize, &T)> {
        let (x0, y0, x1, y1) = bbox;
        let mut done = FxHashSet::with_capacity_and_hasher(64, Default::default());
        let mut result = Vec::with_capacity(16);

        for k in self.get_range(bbox) {
            if let Some(indices) = self.grid.get(&k) {
                for &idx in indices {
                    // Check validity first, then use insert() return to avoid double lookup
                    if !self.objs.contains(&idx) || !done.insert(idx) {
                        continue;
                    }

                    let obj = &self.seq[idx];
                    let obj_bbox = obj.bbox();
                    // Check if object actually intersects the search bbox
                    // Matches Python: exclude if strictly non-overlapping
                    if obj_bbox.2 <= x0 || x1 <= obj_bbox.0 || obj_bbox.3 <= y0 || y1 <= obj_bbox.1
                    {
                        continue;
                    }
                    result.push((idx, obj));
                }
            }
        }
        result
    }

    /// Returns the number of objects in the plane.
    pub fn len(&self) -> usize {
        self.objs.len()
    }

    /// Returns true if the plane is empty.
    pub fn is_empty(&self) -> bool {
        self.objs.is_empty()
    }

    /// Returns true if the object is in the plane.
    pub fn contains(&self, obj: &T) -> bool
    where
        T: PartialEq,
    {
        self.seq
            .iter()
            .enumerate()
            .any(|(i, o)| o == obj && self.objs.contains(&i))
    }

    /// Returns an iterator over all objects in the plane.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.seq
            .iter()
            .enumerate()
            .filter(|(i, _)| self.objs.contains(i))
            .map(|(_, obj)| obj)
    }

    /// Returns an iterator over all objects with their indices in the plane.
    /// The index is the position in the internal seq (insertion order).
    pub fn iter_with_indices(&self) -> impl Iterator<Item = (usize, &T)> {
        self.seq
            .iter()
            .enumerate()
            .filter(|(i, _)| self.objs.contains(i))
    }
}

/// Groups every n elements of an iterator into tuples.
pub fn choplist<T, I>(n: usize, seq: I) -> impl Iterator<Item = Vec<T>>
where
    I: IntoIterator<Item = T>,
{
    let mut iter = seq.into_iter();
    std::iter::from_fn(move || {
        let chunk: Vec<T> = iter.by_ref().take(n).collect();
        if chunk.len() == n { Some(chunk) } else { None }
    })
}

/// Unpacks variable-length unsigned integers (big endian).
pub fn nunpack(s: &[u8], default: u64) -> u64 {
    if s.is_empty() {
        return default;
    }
    let mut result: u64 = 0;
    for &byte in s {
        result = (result << 8) | (byte as u64);
    }
    result
}

/// PDFDocEncoding table - maps bytes 0-255 to Unicode code points.
const PDF_DOC_ENCODING: [u32; 256] = [
    0x0000, 0x0001, 0x0002, 0x0003, 0x0004, 0x0005, 0x0006, 0x0007, 0x0008, 0x0009, 0x000A, 0x000B,
    0x000C, 0x000D, 0x000E, 0x000F, 0x0010, 0x0011, 0x0012, 0x0013, 0x0014, 0x0015, 0x0017, 0x0017,
    0x02D8, 0x02C7, 0x02C6, 0x02D9, 0x02DD, 0x02DB, 0x02DA, 0x02DC, 0x0020, 0x0021, 0x0022, 0x0023,
    0x0024, 0x0025, 0x0026, 0x0027, 0x0028, 0x0029, 0x002A, 0x002B, 0x002C, 0x002D, 0x002E, 0x002F,
    0x0030, 0x0031, 0x0032, 0x0033, 0x0034, 0x0035, 0x0036, 0x0037, 0x0038, 0x0039, 0x003A, 0x003B,
    0x003C, 0x003D, 0x003E, 0x003F, 0x0040, 0x0041, 0x0042, 0x0043, 0x0044, 0x0045, 0x0046, 0x0047,
    0x0048, 0x0049, 0x004A, 0x004B, 0x004C, 0x004D, 0x004E, 0x004F, 0x0050, 0x0051, 0x0052, 0x0053,
    0x0054, 0x0055, 0x0056, 0x0057, 0x0058, 0x0059, 0x005A, 0x005B, 0x005C, 0x005D, 0x005E, 0x005F,
    0x0060, 0x0061, 0x0062, 0x0063, 0x0064, 0x0065, 0x0066, 0x0067, 0x0068, 0x0069, 0x006A, 0x006B,
    0x006C, 0x006D, 0x006E, 0x006F, 0x0070, 0x0071, 0x0072, 0x0073, 0x0074, 0x0075, 0x0076, 0x0077,
    0x0078, 0x0079, 0x007A, 0x007B, 0x007C, 0x007D, 0x007E, 0x0000, 0x2022, 0x2020, 0x2021, 0x2026,
    0x2014, 0x2013, 0x0192, 0x2044, 0x2039, 0x203A, 0x2212, 0x2030, 0x201E, 0x201C, 0x201D, 0x2018,
    0x2019, 0x201A, 0x2122, 0xFB01, 0xFB02, 0x0141, 0x0152, 0x0160, 0x0178, 0x017D, 0x0131, 0x0142,
    0x0153, 0x0161, 0x017E, 0x0000, 0x20AC, 0x00A1, 0x00A2, 0x00A3, 0x00A4, 0x00A5, 0x00A6, 0x00A7,
    0x00A8, 0x00A9, 0x00AA, 0x00AB, 0x00AC, 0x0000, 0x00AE, 0x00AF, 0x00B0, 0x00B1, 0x00B2, 0x00B3,
    0x00B4, 0x00B5, 0x00B6, 0x00B7, 0x00B8, 0x00B9, 0x00BA, 0x00BB, 0x00BC, 0x00BD, 0x00BE, 0x00BF,
    0x00C0, 0x00C1, 0x00C2, 0x00C3, 0x00C4, 0x00C5, 0x00C6, 0x00C7, 0x00C8, 0x00C9, 0x00CA, 0x00CB,
    0x00CC, 0x00CD, 0x00CE, 0x00CF, 0x00D0, 0x00D1, 0x00D2, 0x00D3, 0x00D4, 0x00D5, 0x00D6, 0x00D7,
    0x00D8, 0x00D9, 0x00DA, 0x00DB, 0x00DC, 0x00DD, 0x00DE, 0x00DF, 0x00E0, 0x00E1, 0x00E2, 0x00E3,
    0x00E4, 0x00E5, 0x00E6, 0x00E7, 0x00E8, 0x00E9, 0x00EA, 0x00EB, 0x00EC, 0x00ED, 0x00EE, 0x00EF,
    0x00F0, 0x00F1, 0x00F2, 0x00F3, 0x00F4, 0x00F5, 0x00F6, 0x00F7, 0x00F8, 0x00F9, 0x00FA, 0x00FB,
    0x00FC, 0x00FD, 0x00FE, 0x00FF,
];

/// Decodes a PDFDocEncoding string to Unicode.
/// If the string starts with UTF-16BE BOM (0xFEFF), decode as UTF-16BE.
pub fn decode_text(s: &[u8]) -> String {
    if s.len() >= 2 && s[0] == 0xFE && s[1] == 0xFF {
        // UTF-16BE with BOM
        let u16_chars: Vec<u16> = s[2..]
            .chunks(2)
            .filter_map(|chunk| {
                if chunk.len() == 2 {
                    Some(((chunk[0] as u16) << 8) | (chunk[1] as u16))
                } else {
                    None
                }
            })
            .collect();
        String::from_utf16_lossy(&u16_chars)
    } else {
        // PDFDocEncoding
        s.iter()
            .filter_map(|&c| char::from_u32(PDF_DOC_ENCODING[c as usize]))
            .collect()
    }
}

const ROMAN_ONES: [char; 4] = ['i', 'x', 'c', 'm'];
const ROMAN_FIVES: [char; 3] = ['v', 'l', 'd'];

/// Formats a number as lowercase Roman numerals.
///
/// # Panics
/// Panics if value is not in range 1..4000.
pub fn format_int_roman(value: u32) -> String {
    assert!(value > 0 && value < 4000, "value must be in range 1..4000");

    let mut result = String::new();
    let mut value = value;
    let mut index = 0;

    while value != 0 {
        let remainder = value % 10;
        value /= 10;

        let part = if remainder == 9 {
            format!("{}{}", ROMAN_ONES[index], ROMAN_ONES[index + 1])
        } else if remainder == 4 {
            format!("{}{}", ROMAN_ONES[index], ROMAN_FIVES[index])
        } else {
            let over_five = remainder >= 5;
            let r = if over_five { remainder - 5 } else { remainder };
            let ones: String = std::iter::repeat_n(ROMAN_ONES[index], r as usize).collect();
            if over_five {
                format!("{}{}", ROMAN_FIVES[index], ones)
            } else {
                ones
            }
        };
        result.insert_str(0, &part);
        index += 1;
    }

    result
}

/// Formats a number as lowercase letters a-z, aa-zz, etc.
///
/// # Panics
/// Panics if value is 0.
pub fn format_int_alpha(value: u32) -> String {
    assert!(value > 0, "value must be positive");

    let mut result = Vec::new();
    let mut value = value;

    while value != 0 {
        let remainder = ((value - 1) % 26) as u8;
        value = (value - 1) / 26;
        result.push((b'a' + remainder) as char);
    }

    result.reverse();
    result.into_iter().collect()
}

/// Shortens a string to a maximum size, inserting "..." in the middle if needed.
pub fn shorten_str(s: &str, size: usize) -> String {
    if size < 7 {
        return s.chars().take(size).collect();
    }
    if s.len() > size {
        let length = (size - 5) / 2;
        let start: String = s.chars().take(length).collect();
        let end: String = s.chars().skip(s.len() - length).collect();
        format!("{} ... {}", start, end)
    } else {
        s.to_string()
    }
}

/// Computes a minimal rectangle that covers all the points.
pub fn get_bound<I: IntoIterator<Item = Point>>(pts: I) -> Rect {
    let mut x0 = INF_F64;
    let mut y0 = INF_F64;
    let mut x1 = -INF_F64;
    let mut y1 = -INF_F64;

    for (x, y) in pts {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }

    (x0, y0, x1, y1)
}

/// Checks if a value is a number (always true for f64).
#[inline]
pub fn isnumber<T: num_traits::Num>(_x: T) -> bool {
    true
}

/// Eliminates duplicated elements from an iterator.
pub fn uniq<T: Eq + Hash + Clone>(objs: impl IntoIterator<Item = T>) -> Vec<T> {
    let mut done = HashSet::new();
    let mut result = Vec::new();
    for obj in objs {
        if !done.contains(&obj) {
            done.insert(obj.clone());
            result.push(obj);
        }
    }
    result
}

/// Splits an iterator into two vectors according to a predicate.
pub fn fsplit<T>(pred: impl Fn(&T) -> bool, objs: impl IntoIterator<Item = T>) -> (Vec<T>, Vec<T>) {
    let mut t = Vec::new();
    let mut f = Vec::new();
    for obj in objs {
        if pred(&obj) {
            t.push(obj);
        } else {
            f.push(obj);
        }
    }
    (t, f)
}

/// Picks the object where func(obj) has the highest value.
pub fn pick<T, F>(seq: impl IntoIterator<Item = T>, func: F) -> Option<T>
where
    F: Fn(&T) -> f64,
{
    let mut max_score: Option<f64> = None;
    let mut max_obj: Option<T> = None;

    for obj in seq {
        let score = func(&obj);
        if max_score.is_none() || score > max_score.unwrap() {
            max_score = Some(score);
            max_obj = Some(obj);
        }
    }

    max_obj
}

/// Formats a bounding box as a comma-separated string.
pub fn bbox2str(bbox: Rect) -> String {
    let (x0, y0, x1, y1) = bbox;
    format!("{:.3},{:.3},{:.3},{:.3}", x0, y0, x1, y1)
}

/// Formats a matrix as a string.
pub fn matrix2str(m: Matrix) -> String {
    let (a, b, c, d, e, f) = m;
    format!("[{:.2},{:.2},{:.2},{:.2}, ({:.2},{:.2})]", a, b, c, d, e, f)
}

/// Encodes a string for SGML/XML/HTML by escaping special characters.
///
/// Returns `Cow::Borrowed` if no escaping needed (zero allocation),
/// or `Cow::Owned` with escaped string (single allocation).
pub fn enc(x: &str) -> Cow<'_, str> {
    html_escape::encode_quoted_attribute(x)
}

/// Make a string compatible for output (matching Python's make_compat_str).
///
/// Removes null bytes and replaces invalid UTF-8 sequences with replacement character.
pub fn make_compat_str(s: &str) -> String {
    s.chars().filter(|&c| c != '\0').collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mult_matrix_identity() {
        let identity = MATRIX_IDENTITY;
        assert_eq!(mult_matrix(identity, identity), identity);
    }

    #[test]
    fn test_apply_matrix_pt_identity() {
        let identity = MATRIX_IDENTITY;
        assert_eq!(apply_matrix_pt(identity, (5.0, 10.0)), (5.0, 10.0));
    }

    #[test]
    fn test_choplist() {
        let v: Vec<i32> = vec![1, 2, 3, 4, 5, 6];
        let result: Vec<Vec<i32>> = choplist(2, v).collect();
        assert_eq!(result, vec![vec![1, 2], vec![3, 4], vec![5, 6]]);
    }

    #[test]
    fn test_nunpack() {
        assert_eq!(nunpack(&[], 0), 0);
        assert_eq!(nunpack(&[1], 0), 1);
        assert_eq!(nunpack(&[1, 2], 0), 258);
    }
}
