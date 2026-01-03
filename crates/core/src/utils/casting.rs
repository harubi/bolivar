//! Safe type conversion utilities for PDF objects.
//!
//! Port of pdfminer.six pdfminer/casting.py

use crate::pdftypes::PDFObject;

/// Type alias for a 3-tuple of floats (e.g., RGB color)
pub type FloatTriple = (f64, f64, f64);

/// Type alias for a 4-tuple of floats (e.g., CMYK color, rectangle)
pub type FloatQuadruple = (f64, f64, f64, f64);

/// Type alias for a 6-tuple of floats (transformation matrix)
pub type Matrix = (f64, f64, f64, f64, f64, f64);

/// Type alias for rectangle (same as FloatQuadruple)
pub type Rect = FloatQuadruple;

/// Safely convert a PDFObject to an integer.
///
/// Returns `Some(i64)` if the object is an Int, `None` otherwise.
pub fn safe_int(obj: &PDFObject) -> Option<i64> {
    match obj {
        PDFObject::Int(n) => Some(*n),
        _ => None,
    }
}

/// Safely convert a PDFObject to a float.
///
/// Returns `Some(f64)` if the object is numeric (Int, Real, or parseable String), `None` otherwise.
/// For String inputs, attempts to parse the UTF-8 string as a float.
/// Returns `None` for values that would overflow f64 (infinity).
pub fn safe_float(obj: &PDFObject) -> Option<f64> {
    match obj {
        PDFObject::Int(n) => Some(*n as f64),
        PDFObject::Real(n) => Some(*n),
        PDFObject::String(bytes) => {
            // Try to parse string as float, matching Python's safe_float behavior
            let s = std::str::from_utf8(bytes).ok()?;
            let f = s.parse::<f64>().ok()?;
            // Return None for infinity (overflow), matching Python behavior
            if f.is_infinite() { None } else { Some(f) }
        }
        _ => None,
    }
}

/// Safely create a transformation matrix from 6 PDFObjects.
///
/// Returns `Some(Matrix)` if all objects can be converted to floats, `None` otherwise.
pub fn safe_matrix(
    a: &PDFObject,
    b: &PDFObject,
    c: &PDFObject,
    d: &PDFObject,
    e: &PDFObject,
    f: &PDFObject,
) -> Option<Matrix> {
    let a_f = safe_float(a)?;
    let b_f = safe_float(b)?;
    let c_f = safe_float(c)?;
    let d_f = safe_float(d)?;
    let e_f = safe_float(e)?;
    let f_f = safe_float(f)?;
    Some((a_f, b_f, c_f, d_f, e_f, f_f))
}

/// Safely create an RGB color triple from 3 PDFObjects.
///
/// Returns `Some((r, g, b))` if all objects can be converted to floats, `None` otherwise.
pub fn safe_rgb(r: &PDFObject, g: &PDFObject, b: &PDFObject) -> Option<FloatTriple> {
    safe_float_triple(r, g, b)
}

/// Safely create a CMYK color quadruple from 4 PDFObjects.
///
/// Returns `Some((c, m, y, k))` if all objects can be converted to floats, `None` otherwise.
pub fn safe_cmyk(
    c: &PDFObject,
    m: &PDFObject,
    y: &PDFObject,
    k: &PDFObject,
) -> Option<FloatQuadruple> {
    safe_float_quadruple(c, m, y, k)
}

/// Safely create a rectangle from 4 PDFObjects.
///
/// Returns `Some((x0, y0, x1, y1))` if all objects can be converted to floats, `None` otherwise.
pub fn safe_rect(a: &PDFObject, b: &PDFObject, c: &PDFObject, d: &PDFObject) -> Option<Rect> {
    safe_float_quadruple(a, b, c, d)
}

/// Safely convert an array PDFObject to a rectangle.
///
/// Extracts up to 4 elements from an Array and converts to a rectangle.
/// Returns `None` if:
/// - The object is not an Array
/// - The array has fewer than 4 elements
/// - Any of the first 4 elements cannot be converted to float
pub fn safe_rect_list(obj: &PDFObject) -> Option<Rect> {
    let arr = match obj {
        PDFObject::Array(arr) => arr,
        _ => return None,
    };

    if arr.len() < 4 {
        return None;
    }

    safe_rect(&arr[0], &arr[1], &arr[2], &arr[3])
}

/// Internal helper to create a float triple.
fn safe_float_triple(a: &PDFObject, b: &PDFObject, c: &PDFObject) -> Option<FloatTriple> {
    let a_f = safe_float(a)?;
    let b_f = safe_float(b)?;
    let c_f = safe_float(c)?;
    Some((a_f, b_f, c_f))
}

/// Internal helper to create a float quadruple.
fn safe_float_quadruple(
    a: &PDFObject,
    b: &PDFObject,
    c: &PDFObject,
    d: &PDFObject,
) -> Option<FloatQuadruple> {
    let a_f = safe_float(a)?;
    let b_f = safe_float(b)?;
    let c_f = safe_float(c)?;
    let d_f = safe_float(d)?;
    Some((a_f, b_f, c_f, d_f))
}
