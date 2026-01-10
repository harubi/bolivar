//! PostScript tokenizer and stack parser.
//!
//! Port of pdfminer.six psparser.py

use crate::error::{PdfError, Result};
use std::collections::HashMap;
use std::rc::Rc;
use std::simd::prelude::*;

/// A PostScript literal name.
///
/// Literals are case sensitive and denoted by a preceding
/// slash sign (e.g. /Name). Used as identifiers such as
/// variable names, property names and dictionary keys.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PSLiteral {
    name: String,
}

impl PSLiteral {
    /// Create a new literal with the given name.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }

    /// Get the literal's name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl std::fmt::Display for PSLiteral {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "/{}", self.name)
    }
}

/// PDF/PostScript keyword enum. Known operators are zero-allocation variants.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Keyword {
    // Structural
    ArrayStart, // [
    ArrayEnd,   // ]
    DictStart,  // <<
    DictEnd,    // >>
    BraceOpen,  // {
    BraceClose, // }

    // Primitives
    True,
    False,
    Null,

    // Object structure
    Obj,
    EndObj,
    R,
    Stream,
    EndStream,
    Xref,
    Trailer,
    StartXref,

    // Graphics state
    Q,  // save (uppercase Q)
    Qq, // restore (lowercase q)
    Cm, // concat matrix
    Ww, // line width (lowercase w)
    J,  // line cap (uppercase J)
    Jj, // line join (lowercase j)
    M,  // miter limit
    D,  // dash pattern
    Ri, // rendering intent
    I,  // flatness
    Gs, // graphics state dict

    // Path construction
    Mm, // moveto (lowercase m)
    L,  // lineto
    C,  // curveto
    V,
    Y,
    H,  // closepath
    Re, // rectangle

    // Path painting
    S,      // stroke (uppercase)
    Ss,     // close+stroke (lowercase s)
    F,      // fill (uppercase)
    Ff,     // fill (lowercase f)
    FStar,  // f*
    B,      // fill+stroke
    BStar,  // B*
    Bb,     // close+fill+stroke (lowercase b)
    BbStar, // b*
    N,      // end path

    // Clipping
    WClip, // W (clip)
    WStar, // W*

    // Text object
    BT,
    ET,

    // Text state
    Tc,
    Tw,
    Tz,
    TL,
    Tf,
    Tr,
    Ts,

    // Text positioning
    Td,
    TD,
    Tm,
    TStar, // T*

    // Text showing
    Tj,
    TJ,
    Quote,       // '
    DoubleQuote, // "

    // Color
    CS,
    Cs, // lowercase
    SC,
    SCN,
    Sc,  // lowercase
    Scn, // lowercase
    G,
    Gg, // lowercase g
    RG,
    Rg, // lowercase
    K,
    Kk, // lowercase k

    // XObject
    Do,

    // Inline image
    BI,
    ID,
    EI,

    // Marked content
    MP,
    DP,
    BMC,
    BDC,
    EMC,

    // Missing PDF operators
    Sh, // shading
    D0, // Type3 glyph width
    D1, // Type3 glyph width + bbox
    BX, // begin compatibility
    EX, // end compatibility

    // CMap structure
    BeginCMap,
    EndCMap,
    UseCMap,
    BeginCodeSpaceRange,
    EndCodeSpaceRange,
    BeginBfChar,
    EndBfChar,
    BeginBfRange,
    EndBfRange,
    BeginCidChar,
    EndCidChar,
    BeginCidRange,
    EndCidRange,
    BeginNotDefChar,
    EndNotDefChar,
    BeginNotDefRange,
    EndNotDefRange,

    // PostScript core
    Begin,
    End,
    Def,
    Bind,

    // PostScript stack
    Dup,
    Exch,
    Pop,
    Index,
    Roll,
    Copy,
    Clear,
    Count,

    // PostScript dictionary
    Dict,
    Get,
    Put,
    Known,
    Where,
    CurrentDict,

    // PostScript control
    If,
    IfElse,
    For,
    Loop,
    Repeat,
    Exit,
    Exec,

    // PostScript array/string
    Array,
    PsString, // "string" (avoid Rust keyword)
    Length,
    GetInterval,
    PutInterval,
    Aload,
    Astore,

    // PostScript font
    DefineFont,
    FindFont,
    MakeFont,
    ScaleFont,
    SetFont,
    CurrentFont,
    FontDirectory,

    // PostScript Type1
    Eexec,
    CurrentFile,
    CloseFile,
    ReadOnly,
    ExecuteOnly,
    NoAccess,

    // PostScript misc
    Mark,
    CountToMark,
    ClearToMark,
    Load,
    Store,
    Save,
    Restore,
    SetGlobal,

    // Unknown (preserves original bytes)
    Unknown(Vec<u8>),
}

impl Keyword {
    pub fn from_bytes(b: &[u8]) -> Self {
        match b {
            // Structural
            b"[" => Self::ArrayStart,
            b"]" => Self::ArrayEnd,
            b"<<" => Self::DictStart,
            b">>" => Self::DictEnd,
            b"{" => Self::BraceOpen,
            b"}" => Self::BraceClose,

            // Primitives
            b"true" => Self::True,
            b"false" => Self::False,
            b"null" => Self::Null,

            // Object structure
            b"obj" => Self::Obj,
            b"endobj" => Self::EndObj,
            b"R" => Self::R,
            b"stream" => Self::Stream,
            b"endstream" => Self::EndStream,
            b"xref" => Self::Xref,
            b"trailer" => Self::Trailer,
            b"startxref" => Self::StartXref,

            // Graphics state
            b"Q" => Self::Q,
            b"q" => Self::Qq,
            b"cm" => Self::Cm,
            b"w" => Self::Ww,
            b"J" => Self::J,
            b"j" => Self::Jj,
            b"M" => Self::M,
            b"d" => Self::D,
            b"ri" => Self::Ri,
            b"i" => Self::I,
            b"gs" => Self::Gs,

            // Path construction
            b"m" => Self::Mm,
            b"l" => Self::L,
            b"c" => Self::C,
            b"v" => Self::V,
            b"y" => Self::Y,
            b"h" => Self::H,
            b"re" => Self::Re,

            // Path painting
            b"S" => Self::S,
            b"s" => Self::Ss,
            b"F" => Self::F,
            b"f" => Self::Ff,
            b"f*" => Self::FStar,
            b"B" => Self::B,
            b"B*" => Self::BStar,
            b"b" => Self::Bb,
            b"b*" => Self::BbStar,
            b"n" => Self::N,

            // Clipping (uppercase W)
            b"W" => Self::WClip,
            b"W*" => Self::WStar,

            // Text object
            b"BT" => Self::BT,
            b"ET" => Self::ET,

            // Text state
            b"Tc" => Self::Tc,
            b"Tw" => Self::Tw,
            b"Tz" => Self::Tz,
            b"TL" => Self::TL,
            b"Tf" => Self::Tf,
            b"Tr" => Self::Tr,
            b"Ts" => Self::Ts,

            // Text positioning
            b"Td" => Self::Td,
            b"TD" => Self::TD,
            b"Tm" => Self::Tm,
            b"T*" => Self::TStar,

            // Text showing
            b"Tj" => Self::Tj,
            b"TJ" => Self::TJ,
            b"'" => Self::Quote,
            b"\"" => Self::DoubleQuote,

            // Color
            b"CS" => Self::CS,
            b"cs" => Self::Cs,
            b"SC" => Self::SC,
            b"SCN" => Self::SCN,
            b"sc" => Self::Sc,
            b"scn" => Self::Scn,
            b"G" => Self::G,
            b"g" => Self::Gg,
            b"RG" => Self::RG,
            b"rg" => Self::Rg,
            b"K" => Self::K,
            b"k" => Self::Kk,

            // XObject
            b"Do" => Self::Do,

            // Inline image
            b"BI" => Self::BI,
            b"ID" => Self::ID,
            b"EI" => Self::EI,

            // Marked content
            b"MP" => Self::MP,
            b"DP" => Self::DP,
            b"BMC" => Self::BMC,
            b"BDC" => Self::BDC,
            b"EMC" => Self::EMC,

            // Missing PDF operators
            b"sh" => Self::Sh,
            b"d0" => Self::D0,
            b"d1" => Self::D1,
            b"BX" => Self::BX,
            b"EX" => Self::EX,

            // CMap structure
            b"begincmap" => Self::BeginCMap,
            b"endcmap" => Self::EndCMap,
            b"usecmap" => Self::UseCMap,
            b"begincodespacerange" => Self::BeginCodeSpaceRange,
            b"endcodespacerange" => Self::EndCodeSpaceRange,
            b"beginbfchar" => Self::BeginBfChar,
            b"endbfchar" => Self::EndBfChar,
            b"beginbfrange" => Self::BeginBfRange,
            b"endbfrange" => Self::EndBfRange,
            b"begincidchar" => Self::BeginCidChar,
            b"endcidchar" => Self::EndCidChar,
            b"begincidrange" => Self::BeginCidRange,
            b"endcidrange" => Self::EndCidRange,
            b"beginnotdefchar" => Self::BeginNotDefChar,
            b"endnotdefchar" => Self::EndNotDefChar,
            b"beginnotdefrange" => Self::BeginNotDefRange,
            b"endnotdefrange" => Self::EndNotDefRange,

            // PostScript core
            b"begin" => Self::Begin,
            b"end" => Self::End,
            b"def" => Self::Def,
            b"bind" => Self::Bind,

            // PostScript stack
            b"dup" => Self::Dup,
            b"exch" => Self::Exch,
            b"pop" => Self::Pop,
            b"index" => Self::Index,
            b"roll" => Self::Roll,
            b"copy" => Self::Copy,
            b"clear" => Self::Clear,
            b"count" => Self::Count,

            // PostScript dictionary
            b"dict" => Self::Dict,
            b"get" => Self::Get,
            b"put" => Self::Put,
            b"known" => Self::Known,
            b"where" => Self::Where,
            b"currentdict" => Self::CurrentDict,

            // PostScript control
            b"if" => Self::If,
            b"ifelse" => Self::IfElse,
            b"for" => Self::For,
            b"loop" => Self::Loop,
            b"repeat" => Self::Repeat,
            b"exit" => Self::Exit,
            b"exec" => Self::Exec,

            // PostScript array/string
            b"array" => Self::Array,
            b"string" => Self::PsString,
            b"length" => Self::Length,
            b"getinterval" => Self::GetInterval,
            b"putinterval" => Self::PutInterval,
            b"aload" => Self::Aload,
            b"astore" => Self::Astore,

            // PostScript font
            b"definefont" => Self::DefineFont,
            b"findfont" => Self::FindFont,
            b"makefont" => Self::MakeFont,
            b"scalefont" => Self::ScaleFont,
            b"setfont" => Self::SetFont,
            b"currentfont" => Self::CurrentFont,
            b"FontDirectory" => Self::FontDirectory,

            // PostScript Type1
            b"eexec" => Self::Eexec,
            b"currentfile" => Self::CurrentFile,
            b"closefile" => Self::CloseFile,
            b"readonly" => Self::ReadOnly,
            b"executeonly" => Self::ExecuteOnly,
            b"noaccess" => Self::NoAccess,

            // PostScript misc
            b"mark" => Self::Mark,
            b"counttomark" => Self::CountToMark,
            b"cleartomark" => Self::ClearToMark,
            b"load" => Self::Load,
            b"store" => Self::Store,
            b"save" => Self::Save,
            b"restore" => Self::Restore,
            b"setglobal" => Self::SetGlobal,

            _ => Self::Unknown(b.to_vec()),
        }
    }

    pub const fn as_bytes(&self) -> &[u8] {
        match self {
            Self::ArrayStart => b"[",
            Self::ArrayEnd => b"]",
            Self::DictStart => b"<<",
            Self::DictEnd => b">>",
            Self::BraceOpen => b"{",
            Self::BraceClose => b"}",
            Self::True => b"true",
            Self::False => b"false",
            Self::Null => b"null",
            Self::Obj => b"obj",
            Self::EndObj => b"endobj",
            Self::R => b"R",
            Self::Stream => b"stream",
            Self::EndStream => b"endstream",
            Self::Xref => b"xref",
            Self::Trailer => b"trailer",
            Self::StartXref => b"startxref",
            Self::Q => b"Q",
            Self::Qq => b"q",
            Self::Cm => b"cm",
            Self::WClip => b"W",
            Self::Ww => b"w",
            Self::J => b"J",
            Self::Jj => b"j",
            Self::M => b"M",
            Self::D => b"d",
            Self::Ri => b"ri",
            Self::I => b"i",
            Self::Gs => b"gs",
            Self::Mm => b"m",
            Self::L => b"l",
            Self::C => b"c",
            Self::V => b"v",
            Self::Y => b"y",
            Self::H => b"h",
            Self::Re => b"re",
            Self::S => b"S",
            Self::Ss => b"s",
            Self::F => b"F",
            Self::Ff => b"f",
            Self::FStar => b"f*",
            Self::B => b"B",
            Self::BStar => b"B*",
            Self::Bb => b"b",
            Self::BbStar => b"b*",
            Self::N => b"n",
            Self::WStar => b"W*",
            Self::BT => b"BT",
            Self::ET => b"ET",
            Self::Tc => b"Tc",
            Self::Tw => b"Tw",
            Self::Tz => b"Tz",
            Self::TL => b"TL",
            Self::Tf => b"Tf",
            Self::Tr => b"Tr",
            Self::Ts => b"Ts",
            Self::Td => b"Td",
            Self::TD => b"TD",
            Self::Tm => b"Tm",
            Self::TStar => b"T*",
            Self::Tj => b"Tj",
            Self::TJ => b"TJ",
            Self::Quote => b"'",
            Self::DoubleQuote => b"\"",
            Self::CS => b"CS",
            Self::Cs => b"cs",
            Self::SC => b"SC",
            Self::SCN => b"SCN",
            Self::Sc => b"sc",
            Self::Scn => b"scn",
            Self::G => b"G",
            Self::Gg => b"g",
            Self::RG => b"RG",
            Self::Rg => b"rg",
            Self::K => b"K",
            Self::Kk => b"k",
            Self::Do => b"Do",
            Self::BI => b"BI",
            Self::ID => b"ID",
            Self::EI => b"EI",
            Self::MP => b"MP",
            Self::DP => b"DP",
            Self::BMC => b"BMC",
            Self::BDC => b"BDC",
            Self::EMC => b"EMC",
            // Missing PDF operators
            Self::Sh => b"sh",
            Self::D0 => b"d0",
            Self::D1 => b"d1",
            Self::BX => b"BX",
            Self::EX => b"EX",
            // CMap structure
            Self::BeginCMap => b"begincmap",
            Self::EndCMap => b"endcmap",
            Self::UseCMap => b"usecmap",
            Self::BeginCodeSpaceRange => b"begincodespacerange",
            Self::EndCodeSpaceRange => b"endcodespacerange",
            Self::BeginBfChar => b"beginbfchar",
            Self::EndBfChar => b"endbfchar",
            Self::BeginBfRange => b"beginbfrange",
            Self::EndBfRange => b"endbfrange",
            Self::BeginCidChar => b"begincidchar",
            Self::EndCidChar => b"endcidchar",
            Self::BeginCidRange => b"begincidrange",
            Self::EndCidRange => b"endcidrange",
            Self::BeginNotDefChar => b"beginnotdefchar",
            Self::EndNotDefChar => b"endnotdefchar",
            Self::BeginNotDefRange => b"beginnotdefrange",
            Self::EndNotDefRange => b"endnotdefrange",
            // PostScript core
            Self::Begin => b"begin",
            Self::End => b"end",
            Self::Def => b"def",
            Self::Bind => b"bind",
            // PostScript stack
            Self::Dup => b"dup",
            Self::Exch => b"exch",
            Self::Pop => b"pop",
            Self::Index => b"index",
            Self::Roll => b"roll",
            Self::Copy => b"copy",
            Self::Clear => b"clear",
            Self::Count => b"count",
            // PostScript dictionary
            Self::Dict => b"dict",
            Self::Get => b"get",
            Self::Put => b"put",
            Self::Known => b"known",
            Self::Where => b"where",
            Self::CurrentDict => b"currentdict",
            // PostScript control
            Self::If => b"if",
            Self::IfElse => b"ifelse",
            Self::For => b"for",
            Self::Loop => b"loop",
            Self::Repeat => b"repeat",
            Self::Exit => b"exit",
            Self::Exec => b"exec",
            // PostScript array/string
            Self::Array => b"array",
            Self::PsString => b"string",
            Self::Length => b"length",
            Self::GetInterval => b"getinterval",
            Self::PutInterval => b"putinterval",
            Self::Aload => b"aload",
            Self::Astore => b"astore",
            // PostScript font
            Self::DefineFont => b"definefont",
            Self::FindFont => b"findfont",
            Self::MakeFont => b"makefont",
            Self::ScaleFont => b"scalefont",
            Self::SetFont => b"setfont",
            Self::CurrentFont => b"currentfont",
            Self::FontDirectory => b"FontDirectory",
            // PostScript Type1
            Self::Eexec => b"eexec",
            Self::CurrentFile => b"currentfile",
            Self::CloseFile => b"closefile",
            Self::ReadOnly => b"readonly",
            Self::ExecuteOnly => b"executeonly",
            Self::NoAccess => b"noaccess",
            // PostScript misc
            Self::Mark => b"mark",
            Self::CountToMark => b"counttomark",
            Self::ClearToMark => b"cleartomark",
            Self::Load => b"load",
            Self::Store => b"store",
            Self::Save => b"save",
            Self::Restore => b"restore",
            Self::SetGlobal => b"setglobal",
            Self::Unknown(bytes) => bytes.as_slice(),
        }
    }
}

/// PostScript token types
#[derive(Debug, Clone, PartialEq)]
pub enum PSToken {
    /// Integer value
    Int(i64),
    /// Floating point value
    Real(f64),
    /// Boolean value
    Bool(bool),
    /// Literal name (e.g., /Name)
    Literal(String),
    /// Keyword/operator (e.g., begin, end, BT)
    Keyword(Keyword),
    /// String (literal or hex)
    String(Vec<u8>),
    /// Array
    Array(Vec<Self>),
    /// Dictionary
    Dict(HashMap<String, Self>),
}

/// Buffer size for reading (matches pdfminer.six)
#[allow(dead_code)]
const BUFSIZ: usize = 4096;
#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
const PS_SIMD_LANES: usize = 32;
#[cfg(not(all(target_arch = "x86_64", target_feature = "avx2")))]
const PS_SIMD_LANES: usize = 16;
const PS_SIMD_FULL_MASK: u64 = (1u64 << PS_SIMD_LANES) - 1;

/// PostScript base parser - performs tokenization
enum PSData<'a> {
    Borrowed(&'a [u8]),
    Shared(Rc<[u8]>),
}

impl<'a> PSData<'a> {
    fn as_slice(&self) -> &[u8] {
        match self {
            PSData::Borrowed(data) => data,
            PSData::Shared(data) => data.as_ref(),
        }
    }
}

pub struct PSBaseParser<'a> {
    data: PSData<'a>,
    pos: usize,
    /// Current token position
    token_pos: usize,
}

/// Lexer specialized for PDF content streams.
pub struct ContentLexer<'a> {
    data: PSData<'a>,
    pos: usize,
}

impl<'a> PSBaseParser<'a> {
    pub const fn new(data: &'a [u8]) -> Self {
        Self {
            data: PSData::Borrowed(data),
            pos: 0,
            token_pos: 0,
        }
    }

    /// Create a parser from a raw byte slice (copies into shared storage).
    pub fn from_bytes(data: &[u8]) -> PSBaseParser<'static> {
        PSBaseParser::new_shared(Rc::from(data))
    }

    /// Current position in stream
    pub const fn tell(&self) -> usize {
        self.pos
    }

    /// Set current position in stream.
    pub const fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
        self.token_pos = pos;
    }

    /// Get remaining unparsed data
    pub fn remaining(&self) -> &[u8] {
        &self.data.as_slice()[self.pos..]
    }

    /// Check if at end of data
    fn at_end(&self) -> bool {
        self.pos >= self.data.as_slice().len()
    }

    /// Peek at current byte without advancing
    fn peek(&self) -> Option<u8> {
        self.data.as_slice().get(self.pos).copied()
    }

    /// Peek at byte at offset from current position
    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.data.as_slice().get(self.pos + offset).copied()
    }

    /// Advance position by one
    fn advance(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.pos += 1;
        Some(b)
    }

    /// Check if byte is whitespace
    const fn is_whitespace(b: u8) -> bool {
        matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'\x00' | b'\x0c')
    }

    /// Check if byte is delimiter
    const fn is_delimiter(b: u8) -> bool {
        matches!(
            b,
            b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
        )
    }

    /// Check if byte ends a keyword
    const fn is_keyword_end(b: u8) -> bool {
        Self::is_whitespace(b) || Self::is_delimiter(b)
    }

    fn find_keyword_end_simd(data: &[u8]) -> usize {
        if data.is_empty() {
            return 0;
        }
        let mut i = 0;
        let prefix_len = data.len().min(8);
        while i < prefix_len {
            if Self::is_keyword_end(data[i]) {
                return i;
            }
            i += 1;
        }

        if data.len() - i < PS_SIMD_LANES {
            while i < data.len() {
                if Self::is_keyword_end(data[i]) {
                    return i;
                }
                i += 1;
            }
            return data.len();
        }

        type V = Simd<u8, { PS_SIMD_LANES }>;
        let (prefix, middle, suffix) = data[i..].as_simd::<{ PS_SIMD_LANES }>();

        let mut offset = i;
        for (idx, &b) in prefix.iter().enumerate() {
            if Self::is_keyword_end(b) {
                return offset + idx;
            }
        }
        offset += prefix.len();

        let ws_space = V::splat(b' ');
        let ws_tab = V::splat(b'\t');
        let ws_lf = V::splat(b'\n');
        let ws_cr = V::splat(b'\r');
        let ws_ff = V::splat(0x0c);
        let ws_nul = V::splat(0x00);

        let d_paren_l = V::splat(b'(');
        let d_paren_r = V::splat(b')');
        let d_lt = V::splat(b'<');
        let d_gt = V::splat(b'>');
        let d_brack_l = V::splat(b'[');
        let d_brack_r = V::splat(b']');
        let d_brace_l = V::splat(b'{');
        let d_brace_r = V::splat(b'}');
        let d_slash = V::splat(b'/');
        let d_pct = V::splat(b'%');

        for chunk in middle.iter() {
            let is_ws = chunk.simd_eq(ws_space)
                | chunk.simd_eq(ws_tab)
                | chunk.simd_eq(ws_lf)
                | chunk.simd_eq(ws_cr)
                | chunk.simd_eq(ws_ff)
                | chunk.simd_eq(ws_nul);
            let is_delim = chunk.simd_eq(d_paren_l)
                | chunk.simd_eq(d_paren_r)
                | chunk.simd_eq(d_lt)
                | chunk.simd_eq(d_gt)
                | chunk.simd_eq(d_brack_l)
                | chunk.simd_eq(d_brack_r)
                | chunk.simd_eq(d_brace_l)
                | chunk.simd_eq(d_brace_r)
                | chunk.simd_eq(d_slash)
                | chunk.simd_eq(d_pct);
            let mask = (is_ws | is_delim).to_bitmask();
            if mask != 0 {
                return offset + mask.trailing_zeros() as usize;
            }
            offset += PS_SIMD_LANES;
        }

        for (idx, &b) in suffix.iter().enumerate() {
            if Self::is_keyword_end(b) {
                return offset + idx;
            }
        }

        data.len()
    }

    fn find_first_non_ws_simd(data: &[u8]) -> usize {
        if data.is_empty() {
            return 0;
        }

        let mut i = 0;
        let prefix_len = data.len().min(8);
        while i < prefix_len {
            if !Self::is_whitespace(data[i]) {
                return i;
            }
            i += 1;
        }

        if data.len() - i < PS_SIMD_LANES {
            while i < data.len() {
                if !Self::is_whitespace(data[i]) {
                    return i;
                }
                i += 1;
            }
            return data.len();
        }

        type V = Simd<u8, { PS_SIMD_LANES }>;
        let (prefix, middle, suffix) = data[i..].as_simd::<{ PS_SIMD_LANES }>();

        let mut offset = i;
        for (idx, &b) in prefix.iter().enumerate() {
            if !Self::is_whitespace(b) {
                return offset + idx;
            }
        }
        offset += prefix.len();

        let ws_space = V::splat(b' ');
        let ws_tab = V::splat(b'\t');
        let ws_lf = V::splat(b'\n');
        let ws_cr = V::splat(b'\r');
        let ws_ff = V::splat(0x0c);
        let ws_nul = V::splat(0x00);

        for chunk in middle.iter() {
            let is_ws = chunk.simd_eq(ws_space)
                | chunk.simd_eq(ws_tab)
                | chunk.simd_eq(ws_lf)
                | chunk.simd_eq(ws_cr)
                | chunk.simd_eq(ws_ff)
                | chunk.simd_eq(ws_nul);
            let mask = is_ws.to_bitmask();
            if mask != PS_SIMD_FULL_MASK {
                let non = (!mask) & PS_SIMD_FULL_MASK;
                return offset + non.trailing_zeros() as usize;
            }
            offset += PS_SIMD_LANES;
        }

        for (idx, &b) in suffix.iter().enumerate() {
            if !Self::is_whitespace(b) {
                return offset + idx;
            }
        }

        data.len()
    }

    /// Skip whitespace and comments
    fn skip_whitespace(&mut self) {
        let data = self.data.as_slice();
        while self.pos < data.len() {
            let b = data[self.pos];
            if b == b'%' {
                self.pos += 1; // consume '%'
                if let Some(offset) = find_line_end(&data[self.pos..]) {
                    self.pos += offset + 1; // consume line ending
                } else {
                    self.pos = data.len();
                }
                continue;
            }
            if !Self::is_whitespace(b) {
                return;
            }
            let offset = Self::find_first_non_ws_simd(&data[self.pos..]);
            self.pos += offset.max(1);
        }
    }

    /// Parse a literal name (/Name)
    fn parse_literal(&mut self) -> Result<PSToken> {
        self.advance(); // Skip '/'
        let mut name = Vec::new();

        while let Some(b) = self.peek() {
            if Self::is_whitespace(b) || Self::is_delimiter(b) {
                break;
            }
            if b == b'#' {
                // Hex escape in name - peek ahead to check for valid hex
                let h1 = self.peek_at(1);
                let h2 = self.peek_at(2);

                if let (Some(c1), Some(c2)) = (h1, h2)
                    && c1.is_ascii_hexdigit()
                    && c2.is_ascii_hexdigit()
                {
                    // Valid 2-digit hex escape
                    self.advance(); // consume #
                    self.advance(); // consume first hex digit
                    self.advance(); // consume second hex digit
                    let hex_str = format!("{}{}", c1 as char, c2 as char);
                    if let Ok(byte) = u8::from_str_radix(&hex_str, 16) {
                        name.push(byte);
                    }
                    continue;
                }
                // Invalid hex escape - skip # and continue
                // (per pdfminer.six behavior: # is dropped, following chars kept)
                self.advance(); // consume #
            } else {
                name.push(self.advance().unwrap());
            }
        }

        Ok(PSToken::Literal(name_from_bytes(&name)))
    }

    /// Parse a number (integer or real)
    fn parse_number(&mut self) -> Result<PSToken> {
        self.parse_number_fast()
    }

    fn parse_number_fast(&mut self) -> Result<PSToken> {
        let data = self.data.as_slice();
        let len = data.len();
        let start = self.pos;
        let mut pos = self.pos;
        let mut negative = false;

        if pos < len {
            match data[pos] {
                b'-' => {
                    negative = true;
                    pos += 1;
                }
                b'+' => {
                    pos += 1;
                }
                _ => {}
            }
        }

        let mut int_part: i64 = 0;
        let mut has_int = false;
        let mut overflow = false;
        while pos < len {
            let c = data[pos];
            if c.is_ascii_digit() {
                has_int = true;
                if let Some(v) = int_part
                    .checked_mul(10)
                    .and_then(|v| v.checked_add((c - b'0') as i64))
                {
                    int_part = v;
                } else {
                    overflow = true;
                }
                pos += 1;
            } else {
                break;
            }
        }

        let mut has_dot = false;
        let mut frac_part: i64 = 0;
        let mut frac_digits: u32 = 0;
        if pos < len && data[pos] == b'.' {
            has_dot = true;
            pos += 1;
            while pos < len {
                let c = data[pos];
                if c.is_ascii_digit() {
                    frac_part = frac_part * 10 + (c - b'0') as i64;
                    frac_digits += 1;
                    pos += 1;
                } else {
                    break;
                }
            }
        }

        if !has_int && frac_digits == 0 {
            self.pos = start;
            return Err(PdfError::TokenError {
                pos: start,
                msg: "invalid number".into(),
            });
        }

        self.pos = pos;
        if overflow {
            let s = std::str::from_utf8(&data[start..pos]).map_err(|_| PdfError::TokenError {
                pos: start,
                msg: "invalid number".into(),
            })?;
            if has_dot {
                let val: f64 = s.parse().map_err(|_| PdfError::TokenError {
                    pos: start,
                    msg: format!("invalid real: {}", s),
                })?;
                return Ok(PSToken::Real(val));
            }
            let val: i64 = s.parse().map_err(|_| PdfError::TokenError {
                pos: start,
                msg: format!("invalid int: {}", s),
            })?;
            return Ok(PSToken::Int(val));
        }

        if has_dot {
            let mut value = int_part as f64;
            if frac_digits > 0 {
                let mut divisor = 1.0;
                for _ in 0..frac_digits {
                    divisor *= 10.0;
                }
                value += (frac_part as f64) / divisor;
            }
            if negative {
                value = -value;
            }
            Ok(PSToken::Real(value))
        } else {
            let value = if negative { -int_part } else { int_part };
            Ok(PSToken::Int(value))
        }
    }

    /// Parse a literal string (...)
    fn parse_string(&mut self) -> Result<PSToken> {
        self.advance(); // Skip '('
        let mut result = Vec::new();
        let mut depth = 1;

        while depth > 0 {
            match self.advance() {
                Some(b'(') => {
                    depth += 1;
                    result.push(b'(');
                }
                Some(b')') => {
                    depth -= 1;
                    if depth > 0 {
                        result.push(b')');
                    }
                }
                Some(b'\\') => {
                    // Escape sequence
                    match self.advance() {
                        Some(b'n') => result.push(b'\n'),
                        Some(b'r') => result.push(b'\r'),
                        Some(b't') => result.push(b'\t'),
                        Some(b'b') => result.push(0x08),
                        Some(b'f') => result.push(0x0c),
                        Some(b'(') => result.push(b'('),
                        Some(b')') => result.push(b')'),
                        Some(b'\\') => result.push(b'\\'),
                        Some(b'\r') => {
                            // Line continuation - skip \r and optional \n
                            if self.peek() == Some(b'\n') {
                                self.advance();
                            }
                        }
                        Some(b'\n') => {
                            // Line continuation - skip newline
                        }
                        Some(c) if c.is_ascii_digit() && c < b'8' => {
                            // Octal escape (1-3 digits)
                            let mut octal = (c - b'0') as u32;
                            for _ in 0..2 {
                                if let Some(d) = self.peek() {
                                    if d.is_ascii_digit() && d < b'8' {
                                        self.advance();
                                        octal = octal * 8 + (d - b'0') as u32;
                                    } else {
                                        break;
                                    }
                                }
                            }
                            result.push((octal & 0xFF) as u8);
                        }
                        Some(c) => {
                            // Unknown escape, just keep the character
                            result.push(c);
                        }
                        None => return Err(PdfError::UnexpectedEof),
                    }
                }
                Some(c) => result.push(c),
                None => return Err(PdfError::UnexpectedEof),
            }
        }

        Ok(PSToken::String(result))
    }

    /// Parse a hex string <...>
    fn parse_hex_string(&mut self) -> Result<PSToken> {
        let (decoded, pos) = decode_hex_string(self.data.as_slice(), self.pos)?;
        self.pos = pos;
        Ok(PSToken::String(decoded))
    }

    /// Parse a keyword
    fn parse_keyword(&mut self) -> Result<PSToken> {
        let start = self.pos;
        let data = self.data.as_slice();
        let offset = Self::find_keyword_end_simd(&data[self.pos..]);
        self.pos += offset;
        let bytes = &data[start..self.pos];

        // Check for boolean literals
        if bytes == b"true" {
            return Ok(PSToken::Bool(true));
        } else if bytes == b"false" {
            return Ok(PSToken::Bool(false));
        }

        Ok(PSToken::Keyword(Keyword::from_bytes(bytes)))
    }

    /// Get next token
    pub fn next_token(&mut self) -> Option<Result<(usize, PSToken)>> {
        self.skip_whitespace();

        if self.at_end() {
            return None;
        }

        self.token_pos = self.pos;
        let b = self.peek()?;

        let result = match b {
            b'/' => self.parse_literal(),
            b'(' => self.parse_string(),
            b'<' => {
                if self.peek_at(1) == Some(b'<') {
                    // Dictionary begin
                    self.advance();
                    self.advance();
                    Ok(PSToken::Keyword(Keyword::DictStart))
                } else {
                    self.parse_hex_string()
                }
            }
            b'>' => {
                if self.peek_at(1) == Some(b'>') {
                    // Dictionary end
                    self.advance();
                    self.advance();
                    Ok(PSToken::Keyword(Keyword::DictEnd))
                } else {
                    // Lone '>' - shouldn't happen in valid PS but handle it
                    self.advance();
                    Ok(PSToken::Keyword(Keyword::Unknown(b">".to_vec())))
                }
            }
            b'[' => {
                self.advance();
                Ok(PSToken::Keyword(Keyword::ArrayStart))
            }
            b']' => {
                self.advance();
                Ok(PSToken::Keyword(Keyword::ArrayEnd))
            }
            b'{' => {
                self.advance();
                Ok(PSToken::Keyword(Keyword::BraceOpen))
            }
            b'}' => {
                self.advance();
                Ok(PSToken::Keyword(Keyword::BraceClose))
            }
            b'+' | b'-' => {
                // Could be number or keyword
                if matches!(self.peek_at(1), Some(c) if c.is_ascii_digit() || c == b'.') {
                    self.parse_number()
                } else {
                    self.parse_keyword()
                }
            }
            b'.' => {
                // Could be number (.5) or keyword
                if matches!(self.peek_at(1), Some(c) if c.is_ascii_digit()) {
                    self.parse_number()
                } else {
                    self.parse_keyword()
                }
            }
            c if c.is_ascii_digit() => self.parse_number(),
            _ => self.parse_keyword(),
        };

        Some(result.map(|token| (self.token_pos, token)))
    }
}

impl<'a> ContentLexer<'a> {
    #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
    const SIMD_LANES: usize = 32;
    #[cfg(not(all(target_arch = "x86_64", target_feature = "avx2")))]
    const SIMD_LANES: usize = 16;
    const SIMD_FULL_MASK: u64 = (1u64 << Self::SIMD_LANES) - 1;

    pub const fn new(data: &'a [u8]) -> Self {
        Self {
            data: PSData::Borrowed(data),
            pos: 0,
        }
    }

    /// Create a lexer from a shared byte slice.
    pub const fn new_shared(data: Rc<[u8]>) -> ContentLexer<'static> {
        ContentLexer {
            data: PSData::Shared(data),
            pos: 0,
        }
    }

    /// Set current position in stream.
    pub const fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// Current position in stream.
    pub const fn tell(&self) -> usize {
        self.pos
    }

    fn data(&self) -> &[u8] {
        self.data.as_slice()
    }

    const fn is_whitespace(b: u8) -> bool {
        matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'\x00' | b'\x0c')
    }

    const fn is_delimiter(b: u8) -> bool {
        matches!(
            b,
            b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
        )
    }

    const fn is_keyword_end(b: u8) -> bool {
        Self::is_whitespace(b) || Self::is_delimiter(b)
    }

    fn skip_whitespace(&mut self) {
        let new_pos = {
            let data = self.data();
            let len = data.len();
            let mut pos = self.pos;
            while pos < len {
                let b = data[pos];
                if b == b'%' {
                    pos += 1;
                    if let Some(offset) = find_line_end(&data[pos..]) {
                        pos += offset + 1;
                    } else {
                        pos = len;
                    }
                    continue;
                }
                if !Self::is_whitespace(b) {
                    break;
                }
                pos += Self::find_first_non_ws(&data[pos..]);
            }
            pos
        };
        self.pos = new_pos;
    }

    fn parse_literal(&mut self) -> Result<PSToken> {
        let data = self.data();
        let len = data.len();
        let mut pos = self.pos + 1; // skip '/'
        let mut name = Vec::with_capacity(16);

        while pos < len {
            let b = data[pos];
            if Self::is_whitespace(b) || Self::is_delimiter(b) {
                break;
            }
            if b == b'#' {
                if pos + 2 < len {
                    let c1 = data[pos + 1];
                    let c2 = data[pos + 2];
                    if let (Some(h1), Some(h2)) = (hex_value(c1), hex_value(c2)) {
                        name.push((h1 << 4) | h2);
                        pos += 3;
                        continue;
                    }
                }
                pos += 1;
                continue;
            }
            name.push(b);
            pos += 1;
        }

        self.pos = pos;
        Ok(PSToken::Literal(name_from_bytes(&name)))
    }

    fn parse_number(&mut self) -> Result<PSToken> {
        let data = self.data();
        let len = data.len();
        let start = self.pos;
        let mut pos = self.pos;
        let mut negative = false;

        if pos < len {
            match data[pos] {
                b'-' => {
                    negative = true;
                    pos += 1;
                }
                b'+' => {
                    pos += 1;
                }
                _ => {}
            }
        }

        let mut int_part: i64 = 0;
        let mut has_int = false;
        while pos < len {
            let c = data[pos];
            if c.is_ascii_digit() {
                has_int = true;
                int_part = int_part * 10 + (c - b'0') as i64;
                pos += 1;
            } else {
                break;
            }
        }

        let mut has_dot = false;
        let mut frac_part: i64 = 0;
        let mut frac_digits: u32 = 0;
        if pos < len && data[pos] == b'.' {
            has_dot = true;
            pos += 1;
            while pos < len {
                let c = data[pos];
                if c.is_ascii_digit() {
                    frac_part = frac_part * 10 + (c - b'0') as i64;
                    frac_digits += 1;
                    pos += 1;
                } else {
                    break;
                }
            }
        }

        if !has_int && frac_digits == 0 {
            self.pos = start;
            return Err(PdfError::TokenError {
                pos: start,
                msg: "invalid number".into(),
            });
        }

        self.pos = pos;
        if has_dot {
            let mut value = int_part as f64;
            if frac_digits > 0 {
                let mut divisor = 1.0;
                for _ in 0..frac_digits {
                    divisor *= 10.0;
                }
                value += (frac_part as f64) / divisor;
            }
            if negative {
                value = -value;
            }
            Ok(PSToken::Real(value))
        } else {
            let value = if negative { -int_part } else { int_part };
            Ok(PSToken::Int(value))
        }
    }

    fn parse_string(&mut self) -> Result<PSToken> {
        let data = self.data();
        let len = data.len();
        let mut pos = self.pos + 1; // skip '('
        let mut depth = 1;
        let mut result = Vec::with_capacity(32);

        while pos < len && depth > 0 {
            let c = data[pos];
            pos += 1;
            match c {
                b'(' => {
                    depth += 1;
                    result.push(b'(');
                }
                b')' => {
                    depth -= 1;
                    if depth > 0 {
                        result.push(b')');
                    }
                }
                b'\\' => {
                    if pos >= len {
                        self.pos = pos;
                        return Err(PdfError::UnexpectedEof);
                    }
                    let esc = data[pos];
                    pos += 1;
                    match esc {
                        b'n' => result.push(b'\n'),
                        b'r' => result.push(b'\r'),
                        b't' => result.push(b'\t'),
                        b'b' => result.push(0x08),
                        b'f' => result.push(0x0c),
                        b'(' => result.push(b'('),
                        b')' => result.push(b')'),
                        b'\\' => result.push(b'\\'),
                        b'\r' => {
                            if pos < len && data[pos] == b'\n' {
                                pos += 1;
                            }
                        }
                        b'\n' => {}
                        c if c.is_ascii_digit() && c < b'8' => {
                            let mut octal = (c - b'0') as u32;
                            for _ in 0..2 {
                                if pos < len {
                                    let d = data[pos];
                                    if d.is_ascii_digit() && d < b'8' {
                                        octal = octal * 8 + (d - b'0') as u32;
                                        pos += 1;
                                    } else {
                                        break;
                                    }
                                }
                            }
                            result.push((octal & 0xFF) as u8);
                        }
                        c => result.push(c),
                    }
                }
                c => result.push(c),
            }
        }

        self.pos = pos;
        if depth > 0 {
            return Err(PdfError::UnexpectedEof);
        }
        Ok(PSToken::String(result))
    }

    fn parse_hex_string(&mut self) -> Result<PSToken> {
        let (decoded, pos) = decode_hex_string(self.data(), self.pos)?;
        self.pos = pos;
        Ok(PSToken::String(decoded))
    }

    fn parse_keyword(&mut self) -> Result<PSToken> {
        let (pos, token) = {
            let data = self.data();
            let start = self.pos;
            let offset = Self::find_keyword_end(&data[start..]);
            let pos = start + offset;
            let bytes = &data[start..pos];
            let token = if bytes == b"true" {
                PSToken::Bool(true)
            } else if bytes == b"false" {
                PSToken::Bool(false)
            } else {
                PSToken::Keyword(Keyword::from_bytes(bytes))
            };
            (pos, token)
        };

        self.pos = pos;
        Ok(token)
    }

    /// Get next token
    pub fn next_token(&mut self) -> Option<Result<(usize, PSToken)>> {
        self.skip_whitespace();
        let data = self.data();
        if self.pos >= data.len() {
            return None;
        }

        let token_pos = self.pos;
        let b = data[self.pos];

        let result = match b {
            b'/' => self.parse_literal(),
            b'(' => self.parse_string(),
            b'<' => {
                if self.pos + 1 < data.len() && data[self.pos + 1] == b'<' {
                    self.pos += 2;
                    Ok(PSToken::Keyword(Keyword::DictStart))
                } else {
                    self.parse_hex_string()
                }
            }
            b'>' => {
                if self.pos + 1 < data.len() && data[self.pos + 1] == b'>' {
                    self.pos += 2;
                    Ok(PSToken::Keyword(Keyword::DictEnd))
                } else {
                    self.pos += 1;
                    Ok(PSToken::Keyword(Keyword::Unknown(b">".to_vec())))
                }
            }
            b'[' => {
                self.pos += 1;
                Ok(PSToken::Keyword(Keyword::ArrayStart))
            }
            b']' => {
                self.pos += 1;
                Ok(PSToken::Keyword(Keyword::ArrayEnd))
            }
            b'{' => {
                self.pos += 1;
                Ok(PSToken::Keyword(Keyword::BraceOpen))
            }
            b'}' => {
                self.pos += 1;
                Ok(PSToken::Keyword(Keyword::BraceClose))
            }
            b'+' | b'-' => {
                let next = data.get(self.pos + 1).copied();
                if matches!(next, Some(c) if c.is_ascii_digit() || c == b'.') {
                    self.parse_number()
                } else {
                    self.parse_keyword()
                }
            }
            b'.' => {
                let next = data.get(self.pos + 1).copied();
                if matches!(next, Some(c) if c.is_ascii_digit()) {
                    self.parse_number()
                } else {
                    self.parse_keyword()
                }
            }
            c if c.is_ascii_digit() => self.parse_number(),
            _ => self.parse_keyword(),
        };

        Some(result.map(|token| (token_pos, token)))
    }

    fn find_first_non_ws(data: &[u8]) -> usize {
        if data.is_empty() {
            return 0;
        }
        let mut i = 0;
        let prefix_len = data.len().min(8);
        while i < prefix_len {
            if !Self::is_whitespace(data[i]) {
                return i;
            }
            i += 1;
        }

        if data.len() - i < Self::SIMD_LANES {
            while i < data.len() {
                if !Self::is_whitespace(data[i]) {
                    return i;
                }
                i += 1;
            }
            return data.len();
        }

        type V = Simd<u8, { ContentLexer::SIMD_LANES }>;
        let (prefix, middle, suffix) = data[i..].as_simd::<{ ContentLexer::SIMD_LANES }>();

        let mut offset = i;
        for (idx, &b) in prefix.iter().enumerate() {
            if !Self::is_whitespace(b) {
                return offset + idx;
            }
        }
        offset += prefix.len();

        let ws_space = V::splat(b' ');
        let ws_tab = V::splat(b'\t');
        let ws_lf = V::splat(b'\n');
        let ws_cr = V::splat(b'\r');
        let ws_ff = V::splat(0x0c);
        let ws_nul = V::splat(0x00);

        for chunk in middle.iter() {
            let is_ws = chunk.simd_eq(ws_space)
                | chunk.simd_eq(ws_tab)
                | chunk.simd_eq(ws_lf)
                | chunk.simd_eq(ws_cr)
                | chunk.simd_eq(ws_ff)
                | chunk.simd_eq(ws_nul);
            let mask = is_ws.to_bitmask();
            if mask != Self::SIMD_FULL_MASK {
                let non = (!mask) & Self::SIMD_FULL_MASK;
                return offset + non.trailing_zeros() as usize;
            }
            offset += Self::SIMD_LANES;
        }

        for (idx, &b) in suffix.iter().enumerate() {
            if !Self::is_whitespace(b) {
                return offset + idx;
            }
        }

        data.len()
    }

    fn find_keyword_end(data: &[u8]) -> usize {
        if data.is_empty() {
            return 0;
        }
        let mut i = 0;
        let prefix_len = data.len().min(8);
        while i < prefix_len {
            if Self::is_keyword_end(data[i]) {
                return i;
            }
            i += 1;
        }

        if data.len() - i < Self::SIMD_LANES {
            while i < data.len() {
                if Self::is_keyword_end(data[i]) {
                    return i;
                }
                i += 1;
            }
            return data.len();
        }

        type V = Simd<u8, { ContentLexer::SIMD_LANES }>;
        let (prefix, middle, suffix) = data[i..].as_simd::<{ ContentLexer::SIMD_LANES }>();

        let mut offset = i;
        for (idx, &b) in prefix.iter().enumerate() {
            if Self::is_keyword_end(b) {
                return offset + idx;
            }
        }
        offset += prefix.len();

        let ws_space = V::splat(b' ');
        let ws_tab = V::splat(b'\t');
        let ws_lf = V::splat(b'\n');
        let ws_cr = V::splat(b'\r');
        let ws_ff = V::splat(0x0c);
        let ws_nul = V::splat(0x00);

        let d_paren_l = V::splat(b'(');
        let d_paren_r = V::splat(b')');
        let d_lt = V::splat(b'<');
        let d_gt = V::splat(b'>');
        let d_brack_l = V::splat(b'[');
        let d_brack_r = V::splat(b']');
        let d_brace_l = V::splat(b'{');
        let d_brace_r = V::splat(b'}');
        let d_slash = V::splat(b'/');
        let d_pct = V::splat(b'%');

        for chunk in middle.iter() {
            let is_ws = chunk.simd_eq(ws_space)
                | chunk.simd_eq(ws_tab)
                | chunk.simd_eq(ws_lf)
                | chunk.simd_eq(ws_cr)
                | chunk.simd_eq(ws_ff)
                | chunk.simd_eq(ws_nul);
            let is_delim = chunk.simd_eq(d_paren_l)
                | chunk.simd_eq(d_paren_r)
                | chunk.simd_eq(d_lt)
                | chunk.simd_eq(d_gt)
                | chunk.simd_eq(d_brack_l)
                | chunk.simd_eq(d_brack_r)
                | chunk.simd_eq(d_brace_l)
                | chunk.simd_eq(d_brace_r)
                | chunk.simd_eq(d_slash)
                | chunk.simd_eq(d_pct);
            let is_end = is_ws | is_delim;
            let mask = is_end.to_bitmask();
            if mask != 0 {
                return offset + mask.trailing_zeros() as usize;
            }
            offset += Self::SIMD_LANES;
        }

        for (idx, &b) in suffix.iter().enumerate() {
            if Self::is_keyword_end(b) {
                return offset + idx;
            }
        }

        data.len()
    }
}

#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
const HEX_SIMD_LANES: usize = 32;
#[cfg(not(all(target_arch = "x86_64", target_feature = "avx2")))]
const HEX_SIMD_LANES: usize = 16;

const fn is_ws_byte(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'\x00' | b'\x0c')
}

fn push_hex_nibble(pending: &mut Option<u8>, out: &mut Vec<u8>, nibble: u8) {
    if let Some(high) = pending.take() {
        out.push((high << 4) | nibble);
    } else {
        *pending = Some(nibble);
    }
}

fn decode_hex_string(data: &[u8], start: usize) -> Result<(Vec<u8>, usize)> {
    let len = data.len();
    if start >= len {
        return Err(PdfError::UnexpectedEof);
    }

    let mut pos = start + 1;
    let mut result = Vec::with_capacity(32);
    let mut pending: Option<u8> = None;
    let mut closed = false;

    while pos < len {
        if len - pos >= HEX_SIMD_LANES {
            type V = Simd<u8, { HEX_SIMD_LANES }>;
            let chunk = V::from_slice(&data[pos..pos + HEX_SIMD_LANES]);

            let d0 = V::splat(b'0');
            let d9 = V::splat(b'9');
            let ua = V::splat(b'A');
            let uf = V::splat(b'F');
            let la = V::splat(b'a');
            let lf = V::splat(b'f');

            let is_digit = chunk.simd_ge(d0) & chunk.simd_le(d9);
            let is_upper = chunk.simd_ge(ua) & chunk.simd_le(uf);
            let is_lower = chunk.simd_ge(la) & chunk.simd_le(lf);
            let is_hex = is_digit | is_upper | is_lower;

            let ws_space = V::splat(b' ');
            let ws_tab = V::splat(b'\t');
            let ws_lf = V::splat(b'\n');
            let ws_cr = V::splat(b'\r');
            let ws_ff = V::splat(0x0c);
            let ws_nul = V::splat(0x00);
            let is_ws = chunk.simd_eq(ws_space)
                | chunk.simd_eq(ws_tab)
                | chunk.simd_eq(ws_lf)
                | chunk.simd_eq(ws_cr)
                | chunk.simd_eq(ws_ff)
                | chunk.simd_eq(ws_nul);

            let gt = V::splat(b'>');
            let is_gt = chunk.simd_eq(gt);

            let allowed = is_hex | is_ws | is_gt;
            let invalid = !allowed;
            let stop = invalid | is_gt;
            let stop_mask = stop.to_bitmask();

            if stop_mask == 0 {
                let lanes = chunk.to_array();
                for c in lanes {
                    if let Some(nibble) = hex_value(c) {
                        push_hex_nibble(&mut pending, &mut result, nibble);
                    }
                }
                pos += HEX_SIMD_LANES;
                continue;
            }

            let stop_index = stop_mask.trailing_zeros() as usize;
            let lanes = chunk.to_array();
            for i in 0..stop_index {
                if let Some(nibble) = hex_value(lanes[i]) {
                    push_hex_nibble(&mut pending, &mut result, nibble);
                }
            }
            pos += stop_index;
            let stop_byte = lanes[stop_index];
            if stop_byte == b'>' {
                pos += 1;
                closed = true;
            }
            break;
        }

        let c = data[pos];
        if c == b'>' {
            pos += 1;
            closed = true;
            break;
        }
        if let Some(nibble) = hex_value(c) {
            push_hex_nibble(&mut pending, &mut result, nibble);
            pos += 1;
            continue;
        }
        if is_ws_byte(c) {
            pos += 1;
            continue;
        }
        break;
    }

    if !closed && pos >= len {
        return Err(PdfError::UnexpectedEof);
    }

    if let Some(nibble) = pending {
        result.push(nibble);
    }

    Ok((result, pos))
}

const fn hex_value(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn find_line_end(data: &[u8]) -> Option<usize> {
    for (i, &b) in data.iter().enumerate() {
        if b == b'\r' || b == b'\n' {
            return Some(i);
        }
    }
    None
}

pub(crate) fn name_from_bytes(bytes: &[u8]) -> String {
    let mut name = String::with_capacity(bytes.len());
    for &b in bytes {
        name.push(char::from(b));
    }
    name
}

impl PSBaseParser<'static> {
    /// Create a parser backed by shared storage.
    pub const fn new_shared(data: Rc<[u8]>) -> Self {
        Self {
            data: PSData::Shared(data),
            pos: 0,
            token_pos: 0,
        }
    }
}

/// A positioned token (position, token)
type PosToken = (usize, PSToken);

/// Context frame: (start_position, context_type, saved_stack)
type ContextFrame = (usize, &'static str, Vec<PosToken>);

/// PostScript stack parser - builds objects from tokens
pub struct PSStackParser<'a> {
    base: PSBaseParser<'a>,
    stack: Vec<PosToken>,
    context: Vec<ContextFrame>,
    results: Vec<PosToken>,
}

impl<'a> PSStackParser<'a> {
    pub const fn new(data: &'a [u8]) -> Self {
        Self {
            base: PSBaseParser::new(data),
            stack: Vec::new(),
            context: Vec::new(),
            results: Vec::new(),
        }
    }

    /// Current position in stream
    pub const fn tell(&self) -> usize {
        self.base.tell()
    }

    /// Push objects onto current stack
    fn push(&mut self, pos: usize, obj: PSToken) {
        self.stack.push((pos, obj));
    }

    /// Start a new context (array, dict, or proc)
    fn start_context(&mut self, pos: usize, ctx_type: &'static str) {
        let old_stack = std::mem::take(&mut self.stack);
        self.context.push((pos, ctx_type, old_stack));
    }

    /// End current context and return objects
    fn end_context(&mut self, ctx_type: &'static str) -> Option<(usize, Vec<PSToken>)> {
        if let Some((pos, saved_type, old_stack)) = self.context.pop() {
            if saved_type == ctx_type {
                let objs: Vec<PSToken> = self.stack.drain(..).map(|(_, o)| o).collect();
                self.stack = old_stack;
                return Some((pos, objs));
            }
            // Type mismatch - restore context
            self.context.push((pos, saved_type, old_stack));
        }
        None
    }

    /// Get next object
    pub fn next_object(&mut self) -> Option<Result<(usize, PSToken)>> {
        while self.results.is_empty() {
            let (pos, token) = match self.base.next_token()? {
                Ok(t) => t,
                Err(e) => return Some(Err(e)),
            };

            match &token {
                PSToken::Keyword(Keyword::ArrayStart) => {
                    self.start_context(pos, "array");
                }
                PSToken::Keyword(Keyword::ArrayEnd) => {
                    if let Some((arr_pos, objs)) = self.end_context("array") {
                        self.push(arr_pos, PSToken::Array(objs));
                    }
                }
                PSToken::Keyword(Keyword::DictStart) => {
                    self.start_context(pos, "dict");
                }
                PSToken::Keyword(Keyword::DictEnd) => {
                    if let Some((dict_pos, objs)) = self.end_context("dict") {
                        // Convert pairs to dictionary
                        let mut dict = HashMap::new();
                        let mut iter = objs.into_iter();
                        while let Some(key) = iter.next() {
                            if let PSToken::Literal(name) = key
                                && let Some(value) = iter.next()
                            {
                                dict.insert(name, value);
                            }
                        }
                        self.push(dict_pos, PSToken::Dict(dict));
                    }
                }
                PSToken::Keyword(Keyword::BraceOpen) => {
                    self.start_context(pos, "proc");
                }
                PSToken::Keyword(Keyword::BraceClose) => {
                    if let Some((proc_pos, objs)) = self.end_context("proc") {
                        self.push(proc_pos, PSToken::Array(objs));
                    }
                }
                PSToken::Int(_)
                | PSToken::Real(_)
                | PSToken::Bool(_)
                | PSToken::Literal(_)
                | PSToken::String(_) => {
                    self.push(pos, token);
                }
                PSToken::Keyword(_) => {
                    // Other keywords trigger flush
                }
                _ => {}
            }

            // If not in a context, flush results
            if self.context.is_empty() {
                self.results.append(&mut self.stack);
            }
        }

        if self.results.is_empty() {
            None
        } else {
            Some(Ok(self.results.remove(0)))
        }
    }
}

impl PSStackParser<'static> {
    /// Create a parser from a raw byte slice (copies into shared storage).
    pub fn from_bytes(data: &[u8]) -> Self {
        Self {
            base: PSBaseParser::from_bytes(data),
            stack: Vec::new(),
            context: Vec::new(),
            results: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyword_from_bytes_known() {
        assert_eq!(Keyword::from_bytes(b"obj"), Keyword::Obj);
        assert_eq!(Keyword::from_bytes(b"endobj"), Keyword::EndObj);
        assert_eq!(Keyword::from_bytes(b"R"), Keyword::R);
        assert_eq!(Keyword::from_bytes(b"stream"), Keyword::Stream);
        assert_eq!(Keyword::from_bytes(b"["), Keyword::ArrayStart);
        assert_eq!(Keyword::from_bytes(b"<<"), Keyword::DictStart);
        assert_eq!(Keyword::from_bytes(b"true"), Keyword::True);
        assert_eq!(Keyword::from_bytes(b"BT"), Keyword::BT);
    }

    #[test]
    fn test_keyword_from_bytes_unknown() {
        assert_eq!(
            Keyword::from_bytes(b"notakeyword"),
            Keyword::Unknown(b"notakeyword".to_vec())
        );
        assert_eq!(Keyword::from_bytes(b""), Keyword::Unknown(vec![]));
    }

    #[test]
    fn test_keyword_as_bytes() {
        assert_eq!(Keyword::Obj.as_bytes(), b"obj");
        assert_eq!(Keyword::ArrayStart.as_bytes(), b"[");
        assert_eq!(Keyword::DictEnd.as_bytes(), b">>");
        assert_eq!(Keyword::BT.as_bytes(), b"BT");
    }

    #[test]
    fn find_keyword_end_simd_matches_scalar() {
        let data = b"hello/world";
        let end = PSBaseParser::find_keyword_end_simd(data);
        assert_eq!(end, 5);
        let data = b"hello world";
        let end = PSBaseParser::find_keyword_end_simd(data);
        assert_eq!(end, 5);
        let data = b"hello";
        let end = PSBaseParser::find_keyword_end_simd(data);
        assert_eq!(end, 5);
    }

    #[test]
    fn parse_number_fast_matches_parse() {
        let mut p = PSBaseParser::new(b"12 -3 4.5 -0.25");
        p.skip_whitespace();
        assert_eq!(p.parse_number_fast().unwrap(), PSToken::Int(12));
        p.skip_whitespace();
        assert_eq!(p.parse_number_fast().unwrap(), PSToken::Int(-3));
        p.skip_whitespace();
        assert_eq!(p.parse_number_fast().unwrap(), PSToken::Real(4.5));
        p.skip_whitespace();
        assert_eq!(p.parse_number_fast().unwrap(), PSToken::Real(-0.25));
    }

    #[test]
    fn decode_hex_string_matches_expected() {
        let data = b"<48656c6c6f 20776f726c64>";
        let (decoded, pos) = decode_hex_string(data, 0).unwrap();
        assert_eq!(decoded, b"Hello world");
        assert_eq!(pos, data.len());
    }

    #[test]
    fn find_first_non_ws_simd_skips_whitespace() {
        let data = b" \t\r\n\x00\x0cA";
        let offset = PSBaseParser::find_first_non_ws_simd(data);
        assert_eq!(offset, 6);
    }
}
