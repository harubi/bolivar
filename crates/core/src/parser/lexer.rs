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
            b"[" => Keyword::ArrayStart,
            b"]" => Keyword::ArrayEnd,
            b"<<" => Keyword::DictStart,
            b">>" => Keyword::DictEnd,
            b"{" => Keyword::BraceOpen,
            b"}" => Keyword::BraceClose,

            // Primitives
            b"true" => Keyword::True,
            b"false" => Keyword::False,
            b"null" => Keyword::Null,

            // Object structure
            b"obj" => Keyword::Obj,
            b"endobj" => Keyword::EndObj,
            b"R" => Keyword::R,
            b"stream" => Keyword::Stream,
            b"endstream" => Keyword::EndStream,
            b"xref" => Keyword::Xref,
            b"trailer" => Keyword::Trailer,
            b"startxref" => Keyword::StartXref,

            // Graphics state
            b"Q" => Keyword::Q,
            b"q" => Keyword::Qq,
            b"cm" => Keyword::Cm,
            b"w" => Keyword::Ww,
            b"J" => Keyword::J,
            b"j" => Keyword::Jj,
            b"M" => Keyword::M,
            b"d" => Keyword::D,
            b"ri" => Keyword::Ri,
            b"i" => Keyword::I,
            b"gs" => Keyword::Gs,

            // Path construction
            b"m" => Keyword::Mm,
            b"l" => Keyword::L,
            b"c" => Keyword::C,
            b"v" => Keyword::V,
            b"y" => Keyword::Y,
            b"h" => Keyword::H,
            b"re" => Keyword::Re,

            // Path painting
            b"S" => Keyword::S,
            b"s" => Keyword::Ss,
            b"F" => Keyword::F,
            b"f" => Keyword::Ff,
            b"f*" => Keyword::FStar,
            b"B" => Keyword::B,
            b"B*" => Keyword::BStar,
            b"b" => Keyword::Bb,
            b"b*" => Keyword::BbStar,
            b"n" => Keyword::N,

            // Clipping (uppercase W)
            b"W" => Keyword::WClip,
            b"W*" => Keyword::WStar,

            // Text object
            b"BT" => Keyword::BT,
            b"ET" => Keyword::ET,

            // Text state
            b"Tc" => Keyword::Tc,
            b"Tw" => Keyword::Tw,
            b"Tz" => Keyword::Tz,
            b"TL" => Keyword::TL,
            b"Tf" => Keyword::Tf,
            b"Tr" => Keyword::Tr,
            b"Ts" => Keyword::Ts,

            // Text positioning
            b"Td" => Keyword::Td,
            b"TD" => Keyword::TD,
            b"Tm" => Keyword::Tm,
            b"T*" => Keyword::TStar,

            // Text showing
            b"Tj" => Keyword::Tj,
            b"TJ" => Keyword::TJ,
            b"'" => Keyword::Quote,
            b"\"" => Keyword::DoubleQuote,

            // Color
            b"CS" => Keyword::CS,
            b"cs" => Keyword::Cs,
            b"SC" => Keyword::SC,
            b"SCN" => Keyword::SCN,
            b"sc" => Keyword::Sc,
            b"scn" => Keyword::Scn,
            b"G" => Keyword::G,
            b"g" => Keyword::Gg,
            b"RG" => Keyword::RG,
            b"rg" => Keyword::Rg,
            b"K" => Keyword::K,
            b"k" => Keyword::Kk,

            // XObject
            b"Do" => Keyword::Do,

            // Inline image
            b"BI" => Keyword::BI,
            b"ID" => Keyword::ID,
            b"EI" => Keyword::EI,

            // Marked content
            b"MP" => Keyword::MP,
            b"DP" => Keyword::DP,
            b"BMC" => Keyword::BMC,
            b"BDC" => Keyword::BDC,
            b"EMC" => Keyword::EMC,

            // Missing PDF operators
            b"sh" => Keyword::Sh,
            b"d0" => Keyword::D0,
            b"d1" => Keyword::D1,
            b"BX" => Keyword::BX,
            b"EX" => Keyword::EX,

            // CMap structure
            b"begincmap" => Keyword::BeginCMap,
            b"endcmap" => Keyword::EndCMap,
            b"usecmap" => Keyword::UseCMap,
            b"begincodespacerange" => Keyword::BeginCodeSpaceRange,
            b"endcodespacerange" => Keyword::EndCodeSpaceRange,
            b"beginbfchar" => Keyword::BeginBfChar,
            b"endbfchar" => Keyword::EndBfChar,
            b"beginbfrange" => Keyword::BeginBfRange,
            b"endbfrange" => Keyword::EndBfRange,
            b"begincidchar" => Keyword::BeginCidChar,
            b"endcidchar" => Keyword::EndCidChar,
            b"begincidrange" => Keyword::BeginCidRange,
            b"endcidrange" => Keyword::EndCidRange,
            b"beginnotdefchar" => Keyword::BeginNotDefChar,
            b"endnotdefchar" => Keyword::EndNotDefChar,
            b"beginnotdefrange" => Keyword::BeginNotDefRange,
            b"endnotdefrange" => Keyword::EndNotDefRange,

            // PostScript core
            b"begin" => Keyword::Begin,
            b"end" => Keyword::End,
            b"def" => Keyword::Def,
            b"bind" => Keyword::Bind,

            // PostScript stack
            b"dup" => Keyword::Dup,
            b"exch" => Keyword::Exch,
            b"pop" => Keyword::Pop,
            b"index" => Keyword::Index,
            b"roll" => Keyword::Roll,
            b"copy" => Keyword::Copy,
            b"clear" => Keyword::Clear,
            b"count" => Keyword::Count,

            // PostScript dictionary
            b"dict" => Keyword::Dict,
            b"get" => Keyword::Get,
            b"put" => Keyword::Put,
            b"known" => Keyword::Known,
            b"where" => Keyword::Where,
            b"currentdict" => Keyword::CurrentDict,

            // PostScript control
            b"if" => Keyword::If,
            b"ifelse" => Keyword::IfElse,
            b"for" => Keyword::For,
            b"loop" => Keyword::Loop,
            b"repeat" => Keyword::Repeat,
            b"exit" => Keyword::Exit,
            b"exec" => Keyword::Exec,

            // PostScript array/string
            b"array" => Keyword::Array,
            b"string" => Keyword::PsString,
            b"length" => Keyword::Length,
            b"getinterval" => Keyword::GetInterval,
            b"putinterval" => Keyword::PutInterval,
            b"aload" => Keyword::Aload,
            b"astore" => Keyword::Astore,

            // PostScript font
            b"definefont" => Keyword::DefineFont,
            b"findfont" => Keyword::FindFont,
            b"makefont" => Keyword::MakeFont,
            b"scalefont" => Keyword::ScaleFont,
            b"setfont" => Keyword::SetFont,
            b"currentfont" => Keyword::CurrentFont,
            b"FontDirectory" => Keyword::FontDirectory,

            // PostScript Type1
            b"eexec" => Keyword::Eexec,
            b"currentfile" => Keyword::CurrentFile,
            b"closefile" => Keyword::CloseFile,
            b"readonly" => Keyword::ReadOnly,
            b"executeonly" => Keyword::ExecuteOnly,
            b"noaccess" => Keyword::NoAccess,

            // PostScript misc
            b"mark" => Keyword::Mark,
            b"counttomark" => Keyword::CountToMark,
            b"cleartomark" => Keyword::ClearToMark,
            b"load" => Keyword::Load,
            b"store" => Keyword::Store,
            b"save" => Keyword::Save,
            b"restore" => Keyword::Restore,
            b"setglobal" => Keyword::SetGlobal,

            _ => Keyword::Unknown(b.to_vec()),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Keyword::ArrayStart => b"[",
            Keyword::ArrayEnd => b"]",
            Keyword::DictStart => b"<<",
            Keyword::DictEnd => b">>",
            Keyword::BraceOpen => b"{",
            Keyword::BraceClose => b"}",
            Keyword::True => b"true",
            Keyword::False => b"false",
            Keyword::Null => b"null",
            Keyword::Obj => b"obj",
            Keyword::EndObj => b"endobj",
            Keyword::R => b"R",
            Keyword::Stream => b"stream",
            Keyword::EndStream => b"endstream",
            Keyword::Xref => b"xref",
            Keyword::Trailer => b"trailer",
            Keyword::StartXref => b"startxref",
            Keyword::Q => b"Q",
            Keyword::Qq => b"q",
            Keyword::Cm => b"cm",
            Keyword::WClip => b"W",
            Keyword::Ww => b"w",
            Keyword::J => b"J",
            Keyword::Jj => b"j",
            Keyword::M => b"M",
            Keyword::D => b"d",
            Keyword::Ri => b"ri",
            Keyword::I => b"i",
            Keyword::Gs => b"gs",
            Keyword::Mm => b"m",
            Keyword::L => b"l",
            Keyword::C => b"c",
            Keyword::V => b"v",
            Keyword::Y => b"y",
            Keyword::H => b"h",
            Keyword::Re => b"re",
            Keyword::S => b"S",
            Keyword::Ss => b"s",
            Keyword::F => b"F",
            Keyword::Ff => b"f",
            Keyword::FStar => b"f*",
            Keyword::B => b"B",
            Keyword::BStar => b"B*",
            Keyword::Bb => b"b",
            Keyword::BbStar => b"b*",
            Keyword::N => b"n",
            Keyword::WStar => b"W*",
            Keyword::BT => b"BT",
            Keyword::ET => b"ET",
            Keyword::Tc => b"Tc",
            Keyword::Tw => b"Tw",
            Keyword::Tz => b"Tz",
            Keyword::TL => b"TL",
            Keyword::Tf => b"Tf",
            Keyword::Tr => b"Tr",
            Keyword::Ts => b"Ts",
            Keyword::Td => b"Td",
            Keyword::TD => b"TD",
            Keyword::Tm => b"Tm",
            Keyword::TStar => b"T*",
            Keyword::Tj => b"Tj",
            Keyword::TJ => b"TJ",
            Keyword::Quote => b"'",
            Keyword::DoubleQuote => b"\"",
            Keyword::CS => b"CS",
            Keyword::Cs => b"cs",
            Keyword::SC => b"SC",
            Keyword::SCN => b"SCN",
            Keyword::Sc => b"sc",
            Keyword::Scn => b"scn",
            Keyword::G => b"G",
            Keyword::Gg => b"g",
            Keyword::RG => b"RG",
            Keyword::Rg => b"rg",
            Keyword::K => b"K",
            Keyword::Kk => b"k",
            Keyword::Do => b"Do",
            Keyword::BI => b"BI",
            Keyword::ID => b"ID",
            Keyword::EI => b"EI",
            Keyword::MP => b"MP",
            Keyword::DP => b"DP",
            Keyword::BMC => b"BMC",
            Keyword::BDC => b"BDC",
            Keyword::EMC => b"EMC",
            // Missing PDF operators
            Keyword::Sh => b"sh",
            Keyword::D0 => b"d0",
            Keyword::D1 => b"d1",
            Keyword::BX => b"BX",
            Keyword::EX => b"EX",
            // CMap structure
            Keyword::BeginCMap => b"begincmap",
            Keyword::EndCMap => b"endcmap",
            Keyword::UseCMap => b"usecmap",
            Keyword::BeginCodeSpaceRange => b"begincodespacerange",
            Keyword::EndCodeSpaceRange => b"endcodespacerange",
            Keyword::BeginBfChar => b"beginbfchar",
            Keyword::EndBfChar => b"endbfchar",
            Keyword::BeginBfRange => b"beginbfrange",
            Keyword::EndBfRange => b"endbfrange",
            Keyword::BeginCidChar => b"begincidchar",
            Keyword::EndCidChar => b"endcidchar",
            Keyword::BeginCidRange => b"begincidrange",
            Keyword::EndCidRange => b"endcidrange",
            Keyword::BeginNotDefChar => b"beginnotdefchar",
            Keyword::EndNotDefChar => b"endnotdefchar",
            Keyword::BeginNotDefRange => b"beginnotdefrange",
            Keyword::EndNotDefRange => b"endnotdefrange",
            // PostScript core
            Keyword::Begin => b"begin",
            Keyword::End => b"end",
            Keyword::Def => b"def",
            Keyword::Bind => b"bind",
            // PostScript stack
            Keyword::Dup => b"dup",
            Keyword::Exch => b"exch",
            Keyword::Pop => b"pop",
            Keyword::Index => b"index",
            Keyword::Roll => b"roll",
            Keyword::Copy => b"copy",
            Keyword::Clear => b"clear",
            Keyword::Count => b"count",
            // PostScript dictionary
            Keyword::Dict => b"dict",
            Keyword::Get => b"get",
            Keyword::Put => b"put",
            Keyword::Known => b"known",
            Keyword::Where => b"where",
            Keyword::CurrentDict => b"currentdict",
            // PostScript control
            Keyword::If => b"if",
            Keyword::IfElse => b"ifelse",
            Keyword::For => b"for",
            Keyword::Loop => b"loop",
            Keyword::Repeat => b"repeat",
            Keyword::Exit => b"exit",
            Keyword::Exec => b"exec",
            // PostScript array/string
            Keyword::Array => b"array",
            Keyword::PsString => b"string",
            Keyword::Length => b"length",
            Keyword::GetInterval => b"getinterval",
            Keyword::PutInterval => b"putinterval",
            Keyword::Aload => b"aload",
            Keyword::Astore => b"astore",
            // PostScript font
            Keyword::DefineFont => b"definefont",
            Keyword::FindFont => b"findfont",
            Keyword::MakeFont => b"makefont",
            Keyword::ScaleFont => b"scalefont",
            Keyword::SetFont => b"setfont",
            Keyword::CurrentFont => b"currentfont",
            Keyword::FontDirectory => b"FontDirectory",
            // PostScript Type1
            Keyword::Eexec => b"eexec",
            Keyword::CurrentFile => b"currentfile",
            Keyword::CloseFile => b"closefile",
            Keyword::ReadOnly => b"readonly",
            Keyword::ExecuteOnly => b"executeonly",
            Keyword::NoAccess => b"noaccess",
            // PostScript misc
            Keyword::Mark => b"mark",
            Keyword::CountToMark => b"counttomark",
            Keyword::ClearToMark => b"cleartomark",
            Keyword::Load => b"load",
            Keyword::Store => b"store",
            Keyword::Save => b"save",
            Keyword::Restore => b"restore",
            Keyword::SetGlobal => b"setglobal",
            Keyword::Unknown(bytes) => bytes.as_slice(),
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
    Array(Vec<PSToken>),
    /// Dictionary
    Dict(HashMap<String, PSToken>),
}

/// Buffer size for reading (matches pdfminer.six)
#[allow(dead_code)]
const BUFSIZ: usize = 4096;

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
    pub fn new(data: &'a [u8]) -> Self {
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
    pub fn tell(&self) -> usize {
        self.pos
    }

    /// Set current position in stream.
    pub fn set_pos(&mut self, pos: usize) {
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
    fn is_whitespace(b: u8) -> bool {
        matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'\x00' | b'\x0c')
    }

    /// Check if byte is delimiter
    fn is_delimiter(b: u8) -> bool {
        matches!(
            b,
            b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
        )
    }

    /// Check if byte ends a keyword
    fn is_keyword_end(b: u8) -> bool {
        Self::is_whitespace(b) || Self::is_delimiter(b)
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
            self.pos += 1;
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

                if let (Some(c1), Some(c2)) = (h1, h2) {
                    if c1.is_ascii_hexdigit() && c2.is_ascii_hexdigit() {
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
        let start = self.pos;
        let mut has_dot = false;

        // Handle sign
        if matches!(self.peek(), Some(b'+') | Some(b'-')) {
            self.advance();
        }

        // Handle leading dot
        if self.peek() == Some(b'.') {
            has_dot = true;
            self.advance();
        }

        // Parse digits
        while let Some(b) = self.peek() {
            if b.is_ascii_digit() {
                self.advance();
            } else if b == b'.' && !has_dot {
                has_dot = true;
                self.advance();
            } else {
                break;
            }
        }

        let s = std::str::from_utf8(&self.data.as_slice()[start..self.pos]).map_err(|_| {
            PdfError::TokenError {
                pos: start,
                msg: "invalid number".into(),
            }
        })?;

        if has_dot {
            let val: f64 = s.parse().map_err(|_| PdfError::TokenError {
                pos: start,
                msg: format!("invalid real: {}", s),
            })?;
            Ok(PSToken::Real(val))
        } else {
            let val: i64 = s.parse().map_err(|_| PdfError::TokenError {
                pos: start,
                msg: format!("invalid int: {}", s),
            })?;
            Ok(PSToken::Int(val))
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
        self.advance(); // Skip '<'
        let mut result = Vec::new();
        let data = self.data.as_slice();
        let mut pending: Option<u8> = None;

        loop {
            match data.get(self.pos).copied() {
                Some(b'>') => {
                    self.pos += 1;
                    break;
                }
                Some(c) if c.is_ascii_hexdigit() => {
                    self.pos += 1;
                    let nibble = match c {
                        b'0'..=b'9' => c - b'0',
                        b'a'..=b'f' => c - b'a' + 10,
                        b'A'..=b'F' => c - b'A' + 10,
                        _ => 0,
                    };
                    if let Some(high) = pending {
                        result.push((high << 4) | nibble);
                        pending = None;
                    } else {
                        pending = Some(nibble);
                    }
                }
                Some(c) if Self::is_whitespace(c) => {
                    self.pos += 1;
                }
                Some(_) => {
                    // Invalid character in hex string, stop here
                    break;
                }
                None => return Err(PdfError::UnexpectedEof),
            }
        }

        if let Some(nibble) = pending {
            result.push(nibble);
        }

        Ok(PSToken::String(result))
    }

    /// Parse a keyword
    fn parse_keyword(&mut self) -> Result<PSToken> {
        let start = self.pos;

        while let Some(b) = self.peek() {
            if Self::is_keyword_end(b) {
                break;
            }
            self.advance();
        }

        let bytes = &self.data.as_slice()[start..self.pos];

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

    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data: PSData::Borrowed(data),
            pos: 0,
        }
    }

    /// Create a lexer from a shared byte slice.
    pub fn new_shared(data: Rc<[u8]>) -> ContentLexer<'static> {
        ContentLexer {
            data: PSData::Shared(data),
            pos: 0,
        }
    }

    /// Set current position in stream.
    pub fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// Current position in stream.
    pub fn tell(&self) -> usize {
        self.pos
    }

    fn data(&self) -> &[u8] {
        self.data.as_slice()
    }

    fn is_whitespace(b: u8) -> bool {
        matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'\x00' | b'\x0c')
    }

    fn is_delimiter(b: u8) -> bool {
        matches!(
            b,
            b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
        )
    }

    fn is_keyword_end(b: u8) -> bool {
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
        let data = self.data();
        let len = data.len();
        let mut pos = self.pos + 1; // skip '<'
        let mut result = Vec::with_capacity(32);
        let mut pending: Option<u8> = None;
        let mut closed = false;

        while pos < len {
            let c = data[pos];
            if c == b'>' {
                pos += 1;
                closed = true;
                break;
            }
            if c.is_ascii_hexdigit() {
                let nibble = match c {
                    b'0'..=b'9' => c - b'0',
                    b'a'..=b'f' => c - b'a' + 10,
                    b'A'..=b'F' => c - b'A' + 10,
                    _ => 0,
                };
                if let Some(high) = pending {
                    result.push((high << 4) | nibble);
                    pending = None;
                } else {
                    pending = Some(nibble);
                }
                pos += 1;
                continue;
            }
            if Self::is_whitespace(c) {
                pos += 1;
                continue;
            }
            break;
        }

        if !closed && pos >= len {
            self.pos = pos;
            return Err(PdfError::UnexpectedEof);
        }

        if let Some(nibble) = pending {
            result.push(nibble);
        }

        self.pos = pos;
        Ok(PSToken::String(result))
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

fn hex_value(c: u8) -> Option<u8> {
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
    pub fn new_shared(data: Rc<[u8]>) -> Self {
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
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            base: PSBaseParser::new(data),
            stack: Vec::new(),
            context: Vec::new(),
            results: Vec::new(),
        }
    }

    /// Current position in stream
    pub fn tell(&self) -> usize {
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
                            if let PSToken::Literal(name) = key {
                                if let Some(value) = iter.next() {
                                    dict.insert(name, value);
                                }
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
}
