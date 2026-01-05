//! PDF object types.
//!
//! Port of pdfminer.six pdftypes.py

use crate::error::{PdfError, Result};
use bytes::Bytes;
use std::collections::HashMap;

/// PDF Object types - the fundamental value type in PDF.
#[derive(Debug, Clone, PartialEq)]
pub enum PDFObject {
    /// Null object
    Null,
    /// Boolean value
    Bool(bool),
    /// Integer value
    Int(i64),
    /// Real (floating point) value
    Real(f64),
    /// Name object (e.g., /Type, /Font)
    Name(String),
    /// String (byte array)
    String(Vec<u8>),
    /// Array of objects
    Array(Vec<Self>),
    /// Dictionary (name -> object mapping)
    Dict(HashMap<String, Self>),
    /// Stream (dictionary + binary data)
    Stream(Box<PDFStream>),
    /// Indirect object reference
    Ref(PDFObjRef),
}

impl PDFObject {
    /// Check if this is a null object
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Get as boolean
    pub const fn as_bool(&self) -> Result<bool> {
        match self {
            Self::Bool(b) => Ok(*b),
            _ => Err(PdfError::TypeError {
                expected: "bool",
                got: self.type_name(),
            }),
        }
    }

    /// Get as integer
    pub const fn as_int(&self) -> Result<i64> {
        match self {
            Self::Int(n) => Ok(*n),
            _ => Err(PdfError::TypeError {
                expected: "int",
                got: self.type_name(),
            }),
        }
    }

    /// Get as real (float)
    pub const fn as_real(&self) -> Result<f64> {
        match self {
            Self::Real(n) => Ok(*n),
            _ => Err(PdfError::TypeError {
                expected: "real",
                got: self.type_name(),
            }),
        }
    }

    /// Get numeric value (int or real coerced to f64)
    pub const fn as_num(&self) -> Result<f64> {
        match self {
            Self::Int(n) => Ok(*n as f64),
            Self::Real(n) => Ok(*n),
            _ => Err(PdfError::TypeError {
                expected: "number",
                got: self.type_name(),
            }),
        }
    }

    /// Get as name string
    pub fn as_name(&self) -> Result<&str> {
        match self {
            Self::Name(s) => Ok(s),
            _ => Err(PdfError::TypeError {
                expected: "name",
                got: self.type_name(),
            }),
        }
    }

    /// Get as byte string
    pub fn as_string(&self) -> Result<&[u8]> {
        match self {
            Self::String(s) => Ok(s),
            _ => Err(PdfError::TypeError {
                expected: "string",
                got: self.type_name(),
            }),
        }
    }

    /// Get as array
    pub const fn as_array(&self) -> Result<&Vec<Self>> {
        match self {
            Self::Array(arr) => Ok(arr),
            _ => Err(PdfError::TypeError {
                expected: "array",
                got: self.type_name(),
            }),
        }
    }

    /// Get as dictionary
    pub const fn as_dict(&self) -> Result<&HashMap<String, Self>> {
        match self {
            Self::Dict(d) => Ok(d),
            _ => Err(PdfError::TypeError {
                expected: "dict",
                got: self.type_name(),
            }),
        }
    }

    /// Get as stream
    pub fn as_stream(&self) -> Result<&PDFStream> {
        match self {
            Self::Stream(s) => Ok(s),
            _ => Err(PdfError::TypeError {
                expected: "stream",
                got: self.type_name(),
            }),
        }
    }

    /// Get as object reference
    pub const fn as_ref(&self) -> Result<&PDFObjRef> {
        match self {
            Self::Ref(r) => Ok(r),
            _ => Err(PdfError::TypeError {
                expected: "ref",
                got: self.type_name(),
            }),
        }
    }

    /// Get type name for error messages
    const fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Real(_) => "real",
            Self::Name(_) => "name",
            Self::String(_) => "string",
            Self::Array(_) => "array",
            Self::Dict(_) => "dict",
            Self::Stream(_) => "stream",
            Self::Ref(_) => "ref",
        }
    }
}

/// PDF indirect object reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PDFObjRef {
    /// Object ID
    pub objid: u32,
    /// Generation number
    pub genno: u32,
}

impl PDFObjRef {
    /// Create a new object reference.
    pub const fn new(objid: u32, genno: u32) -> Self {
        Self { objid, genno }
    }
}

/// PDF Stream - dictionary attributes + binary data.
#[derive(Debug, Clone, PartialEq)]
pub struct PDFStream {
    /// Stream dictionary attributes
    pub attrs: HashMap<String, PDFObject>,
    /// Raw (possibly encoded) data
    rawdata: Bytes,
    /// Whether rawdata has already been decrypted
    rawdata_decrypted: bool,
    /// Decoded data (lazily populated)
    data: Option<Vec<u8>>,
    /// Object ID (set when stream is part of document)
    pub objid: Option<u32>,
    /// Generation number
    pub genno: Option<u32>,
}

impl PDFStream {
    /// Create a new stream.
    pub fn new(attrs: HashMap<String, PDFObject>, rawdata: impl Into<Bytes>) -> Self {
        Self {
            attrs,
            rawdata: rawdata.into(),
            rawdata_decrypted: false,
            data: None,
            objid: None,
            genno: None,
        }
    }

    /// Set object ID and generation number.
    pub const fn set_objid(&mut self, objid: u32, genno: u32) {
        self.objid = Some(objid);
        self.genno = Some(genno);
    }

    /// Get raw (undecoded) data.
    pub fn get_rawdata(&self) -> &[u8] {
        self.rawdata.as_ref()
    }

    /// Get raw data as shared bytes.
    pub fn rawdata_bytes(&self) -> Bytes {
        self.rawdata.clone()
    }

    /// Check if rawdata has been decrypted already.
    pub const fn rawdata_is_decrypted(&self) -> bool {
        self.rawdata_decrypted
    }

    /// Replace rawdata and mark it as decrypted.
    pub fn set_rawdata_decrypted(&mut self, data: Vec<u8>) {
        self.rawdata = Bytes::from(data);
        self.rawdata_decrypted = true;
        self.data = None;
    }

    /// Get decoded data.
    ///
    /// Note: In a full implementation, this would handle filters like
    /// FlateDecode, LZWDecode, etc. For now, returns raw data.
    pub fn get_data(&self) -> &[u8] {
        self.data
            .as_deref()
            .unwrap_or_else(|| self.rawdata.as_ref())
    }

    /// Check if stream contains a key.
    pub fn contains(&self, name: &str) -> bool {
        self.attrs.contains_key(name)
    }

    /// Get attribute by name.
    pub fn get(&self, name: &str) -> Option<&PDFObject> {
        self.attrs.get(name)
    }

    /// Get attribute, trying multiple names.
    pub fn get_any(&self, names: &[&str]) -> Option<&PDFObject> {
        for name in names {
            if let Some(obj) = self.attrs.get(*name) {
                return Some(obj);
            }
        }
        None
    }
}

// === Type conversion helper functions ===

/// Get integer value from object.
pub fn int_value(obj: &PDFObject) -> Result<i64> {
    obj.as_int()
}

/// Get float value from object.
pub fn float_value(obj: &PDFObject) -> Result<f64> {
    obj.as_real()
}

/// Get numeric value (int or float) from object.
pub fn num_value(obj: &PDFObject) -> Result<f64> {
    obj.as_num()
}

/// Get string value from object.
pub fn str_value(obj: &PDFObject) -> Result<&[u8]> {
    obj.as_string()
}

/// Get list/array value from object.
pub fn list_value(obj: &PDFObject) -> Result<&Vec<PDFObject>> {
    obj.as_array()
}

/// Get dictionary value from object.
pub fn dict_value(obj: &PDFObject) -> Result<&HashMap<String, PDFObject>> {
    obj.as_dict()
}

/// Get stream value from object.
pub fn stream_value(obj: &PDFObject) -> Result<&PDFStream> {
    obj.as_stream()
}
