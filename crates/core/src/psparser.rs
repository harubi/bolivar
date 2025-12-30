//! PostScript tokenizer and stack parser.
//!
//! Port of pdfminer.six psparser.py

use crate::error::{PdfError, Result};
use std::collections::HashMap;
use std::rc::Rc;

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

/// A PostScript keyword.
///
/// Keywords are a special kind of identifier used for operators
/// and procedural objects in the PDF specification.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PSKeyword {
    name: Vec<u8>,
}

impl PSKeyword {
    /// Create a new keyword with the given name.
    pub fn new(name: &[u8]) -> Self {
        Self {
            name: name.to_vec(),
        }
    }

    /// Get the keyword's name as bytes.
    pub fn name(&self) -> &[u8] {
        &self.name
    }

    /// Get the keyword's name as a string if valid UTF-8.
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.name).ok()
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
    Keyword(Vec<u8>),
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
        while let Some(b) = self.peek() {
            if Self::is_whitespace(b) {
                self.advance();
            } else if b == b'%' {
                // Skip comment to end of line
                while let Some(c) = self.advance() {
                    if c == b'\r' || c == b'\n' {
                        break;
                    }
                }
            } else {
                break;
            }
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

        // Try to convert to UTF-8 string
        let name_str = String::from_utf8(name.clone())
            .unwrap_or_else(|_| String::from_utf8_lossy(&name).into_owned());

        Ok(PSToken::Literal(name_str))
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
        let mut hex_chars = Vec::new();

        loop {
            match self.peek() {
                Some(b'>') => {
                    self.advance();
                    break;
                }
                Some(c) if c.is_ascii_hexdigit() => {
                    self.advance();
                    hex_chars.push(c);
                }
                Some(c) if Self::is_whitespace(c) => {
                    self.advance();
                }
                Some(_) => {
                    // Invalid character in hex string, stop here
                    break;
                }
                None => return Err(PdfError::UnexpectedEof),
            }
        }

        // Convert hex to bytes - pairs first, then single chars as single-digit hex
        // (per pdfminer.six: regex `[0-9a-fA-F]{2}|.` converts pairs, then singles)
        let mut result = Vec::new();
        let mut i = 0;
        while i < hex_chars.len() {
            if i + 1 < hex_chars.len() {
                // Two hex digits available - convert as pair
                let hex = std::str::from_utf8(&hex_chars[i..i + 2]).unwrap();
                let byte = u8::from_str_radix(hex, 16).unwrap();
                result.push(byte);
                i += 2;
            } else {
                // Single hex digit - convert as single-digit hex (0-15)
                let hex = std::str::from_utf8(&hex_chars[i..i + 1]).unwrap();
                let byte = u8::from_str_radix(hex, 16).unwrap();
                result.push(byte);
                i += 1;
            }
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

        let keyword = self.data.as_slice()[start..self.pos].to_vec();

        // Check for boolean literals
        if keyword == b"true" {
            return Ok(PSToken::Bool(true));
        } else if keyword == b"false" {
            return Ok(PSToken::Bool(false));
        }

        Ok(PSToken::Keyword(keyword))
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
                    Ok(PSToken::Keyword(b"<<".to_vec()))
                } else {
                    self.parse_hex_string()
                }
            }
            b'>' => {
                if self.peek_at(1) == Some(b'>') {
                    // Dictionary end
                    self.advance();
                    self.advance();
                    Ok(PSToken::Keyword(b">>".to_vec()))
                } else {
                    // Lone '>' - shouldn't happen in valid PS but handle it
                    self.advance();
                    Ok(PSToken::Keyword(b">".to_vec()))
                }
            }
            b'[' => {
                self.advance();
                Ok(PSToken::Keyword(b"[".to_vec()))
            }
            b']' => {
                self.advance();
                Ok(PSToken::Keyword(b"]".to_vec()))
            }
            b'{' => {
                self.advance();
                Ok(PSToken::Keyword(b"{".to_vec()))
            }
            b'}' => {
                self.advance();
                Ok(PSToken::Keyword(b"}".to_vec()))
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
                PSToken::Keyword(kw) if kw == b"[" => {
                    self.start_context(pos, "array");
                }
                PSToken::Keyword(kw) if kw == b"]" => {
                    if let Some((arr_pos, objs)) = self.end_context("array") {
                        self.push(arr_pos, PSToken::Array(objs));
                    }
                }
                PSToken::Keyword(kw) if kw == b"<<" => {
                    self.start_context(pos, "dict");
                }
                PSToken::Keyword(kw) if kw == b">>" => {
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
                PSToken::Keyword(kw) if kw == b"{" => {
                    self.start_context(pos, "proc");
                }
                PSToken::Keyword(kw) if kw == b"}" => {
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
