//! PDF content stream interpreter.
//!
//! Port of pdfminer.six pdfinterp.py - PDFContentParser and PDFResourceManager classes.
//!
//! PDFContentParser parses PDF content streams containing operators
//! like BT, ET, Tm, Tj, as well as inline images (BI/ID/EI).
//!
//! PDFResourceManager facilitates reuse of shared resources such as fonts
//! and color spaces so that large objects are not allocated multiple times.

use crate::cmapdb::{CMap, CMapDB};
use crate::error::{PdfError, Result};
use crate::pdfcolor::{PDFColorSpace, PREDEFINED_COLORSPACE};
use crate::pdftypes::{PDFObject, PDFStream};
use crate::psparser::{Keyword, PSLiteral, PSToken};
use bytes::Bytes;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/// Token types produced by PDFContentParser.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentToken {
    /// An operand (number, string, array, dict, literal)
    Operand(PSToken),
    /// A keyword/operator (BT, ET, Tj, etc.)
    Keyword(Keyword),
    /// An inline image with dictionary and data
    InlineImage {
        dict: HashMap<String, PSToken>,
        data: Vec<u8>,
    },
}

/// Context frame for tracking array/dict/proc construction
#[derive(Debug)]
enum Context {
    Array(usize, Vec<PSToken>),
    Dict(usize, Vec<PSToken>),
    Proc(usize, Vec<PSToken>),
}

struct SegmentedCursor {
    segments: Vec<Bytes>,
    offsets: Vec<usize>,
    seg_index: usize,
    seg_pos: usize,
    total_len: usize,
}

impl SegmentedCursor {
    fn new(mut segments: Vec<Bytes>) -> Self {
        segments.retain(|seg| !seg.is_empty());
        let mut offsets = Vec::with_capacity(segments.len());
        let mut total_len = 0usize;
        for seg in &segments {
            offsets.push(total_len);
            total_len = total_len.saturating_add(seg.len());
        }
        Self {
            segments,
            offsets,
            seg_index: 0,
            seg_pos: 0,
            total_len,
        }
    }

    const fn total_len(&self) -> usize {
        self.total_len
    }

    fn at_end(&self) -> bool {
        self.tell() >= self.total_len
    }

    fn tell(&self) -> usize {
        if self.seg_index >= self.segments.len() {
            self.total_len
        } else {
            self.offsets[self.seg_index] + self.seg_pos
        }
    }

    fn set_pos(&mut self, pos: usize) {
        let pos = pos.min(self.total_len);
        if pos == self.total_len {
            self.seg_index = self.segments.len();
            self.seg_pos = 0;
            return;
        }
        let idx = match self.offsets.binary_search(&pos) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        self.seg_index = idx.min(self.segments.len());
        if self.seg_index >= self.segments.len() {
            self.seg_pos = 0;
            return;
        }
        self.seg_pos = pos.saturating_sub(self.offsets[self.seg_index]);
    }

    fn normalize(&mut self) {
        while self.seg_index < self.segments.len()
            && self.seg_pos >= self.segments[self.seg_index].len()
        {
            self.seg_index += 1;
            self.seg_pos = 0;
        }
    }

    fn current_slice(&self) -> &[u8] {
        if self.seg_index >= self.segments.len() {
            &[]
        } else {
            &self.segments[self.seg_index][self.seg_pos..]
        }
    }

    fn peek(&self) -> Option<u8> {
        self.current_slice().first().copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        let mut idx = self.seg_index;
        let mut pos = self.seg_pos + offset;
        while idx < self.segments.len() {
            let seg = &self.segments[idx];
            if pos < seg.len() {
                return Some(seg[pos]);
            }
            pos = pos.saturating_sub(seg.len());
            idx += 1;
        }
        None
    }

    fn advance(&mut self, mut count: usize) {
        while count > 0 && self.seg_index < self.segments.len() {
            let seg_len = self.segments[self.seg_index].len();
            let remaining = seg_len.saturating_sub(self.seg_pos);
            if count < remaining {
                self.seg_pos += count;
                return;
            }
            count = count.saturating_sub(remaining);
            self.seg_index += 1;
            self.seg_pos = 0;
        }
    }

    fn advance_one(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.advance(1);
        Some(b)
    }

    fn match_bytes(&self, target: &[u8]) -> bool {
        for (i, &b) in target.iter().enumerate() {
            if self.peek_at(i) != Some(b) {
                return false;
            }
        }
        true
    }
}

struct SegmentedContentLexer {
    cursor: SegmentedCursor,
}

impl SegmentedContentLexer {
    fn new(segments: Vec<Bytes>) -> Self {
        Self {
            cursor: SegmentedCursor::new(segments),
        }
    }

    const fn total_len(&self) -> usize {
        self.cursor.total_len()
    }

    fn tell(&self) -> usize {
        self.cursor.tell()
    }

    fn set_pos(&mut self, pos: usize) {
        self.cursor.set_pos(pos);
    }

    fn next_token(&mut self) -> Option<Result<(usize, PSToken)>> {
        self.skip_whitespace();
        if self.cursor.at_end() {
            return None;
        }

        let token_pos = self.cursor.tell();
        let b = self.cursor.peek()?;

        let result = match b {
            b'/' => self.parse_literal(),
            b'(' => self.parse_string(),
            b'<' => {
                if self.cursor.peek_at(1) == Some(b'<') {
                    self.cursor.advance(2);
                    Ok(PSToken::Keyword(Keyword::DictStart))
                } else {
                    self.parse_hex_string()
                }
            }
            b'>' => {
                if self.cursor.peek_at(1) == Some(b'>') {
                    self.cursor.advance(2);
                    Ok(PSToken::Keyword(Keyword::DictEnd))
                } else {
                    self.cursor.advance(1);
                    Ok(PSToken::Keyword(Keyword::Unknown(b">".to_vec())))
                }
            }
            b'[' => {
                self.cursor.advance(1);
                Ok(PSToken::Keyword(Keyword::ArrayStart))
            }
            b']' => {
                self.cursor.advance(1);
                Ok(PSToken::Keyword(Keyword::ArrayEnd))
            }
            b'{' => {
                self.cursor.advance(1);
                Ok(PSToken::Keyword(Keyword::BraceOpen))
            }
            b'}' => {
                self.cursor.advance(1);
                Ok(PSToken::Keyword(Keyword::BraceClose))
            }
            b'+' | b'-' => {
                if matches!(self.cursor.peek_at(1), Some(c) if c.is_ascii_digit() || c == b'.') {
                    self.parse_number(token_pos)
                } else {
                    self.parse_keyword()
                }
            }
            b'.' => {
                if matches!(self.cursor.peek_at(1), Some(c) if c.is_ascii_digit()) {
                    self.parse_number(token_pos)
                } else {
                    self.parse_keyword()
                }
            }
            c if c.is_ascii_digit() => self.parse_number(token_pos),
            _ => self.parse_keyword(),
        };

        Some(result.map(|token| (token_pos, token)))
    }

    fn skip_whitespace(&mut self) {
        loop {
            self.cursor.normalize();
            let slice = self.cursor.current_slice();
            if slice.is_empty() {
                return;
            }
            enum Action {
                Advance(usize),
                Comment(usize),
                ConsumeAll(usize),
            }
            let action = {
                let mut idx = 0usize;
                loop {
                    if idx >= slice.len() {
                        break Action::ConsumeAll(slice.len());
                    }
                    let b = slice[idx];
                    if b == b'%' {
                        break Action::Comment(idx + 1);
                    }
                    if !is_whitespace(b) {
                        break Action::Advance(idx);
                    }
                    idx += 1;
                }
            };
            match action {
                Action::Advance(n) => {
                    self.cursor.advance(n);
                    return;
                }
                Action::Comment(n) => {
                    self.cursor.advance(n);
                    self.skip_comment();
                }
                Action::ConsumeAll(n) => {
                    self.cursor.advance(n);
                }
            }
        }
    }

    fn skip_comment(&mut self) {
        while let Some(b) = self.cursor.advance_one() {
            if b == b'\n' || b == b'\r' {
                break;
            }
        }
    }

    fn parse_literal(&mut self) -> Result<PSToken> {
        self.cursor.advance(1); // skip '/'
        let mut name = Vec::with_capacity(16);

        while let Some(b) = self.cursor.peek() {
            if is_whitespace(b) || is_delimiter(b) {
                break;
            }
            if b == b'#' {
                let c1 = self.cursor.peek_at(1);
                let c2 = self.cursor.peek_at(2);
                if let (Some(h1), Some(h2)) = (c1.and_then(hex_value), c2.and_then(hex_value)) {
                    self.cursor.advance(3);
                    name.push((h1 << 4) | h2);
                    continue;
                }
                self.cursor.advance(1);
                continue;
            }
            name.push(self.cursor.advance_one().unwrap());
        }

        let name = match String::from_utf8(name) {
            Ok(s) => s,
            Err(e) => String::from_utf8_lossy(&e.into_bytes()).into_owned(),
        };
        Ok(PSToken::Literal(name))
    }

    fn parse_number(&mut self, start_pos: usize) -> Result<PSToken> {
        let mut negative = false;
        if matches!(self.cursor.peek(), Some(b'-')) {
            negative = true;
            self.cursor.advance(1);
        } else if matches!(self.cursor.peek(), Some(b'+')) {
            self.cursor.advance(1);
        }

        let mut int_part: i64 = 0;
        let mut has_int = false;
        while let Some(b) = self.cursor.peek() {
            if b.is_ascii_digit() {
                has_int = true;
                int_part = int_part * 10 + (b - b'0') as i64;
                self.cursor.advance(1);
            } else {
                break;
            }
        }

        let mut has_dot = false;
        let mut frac_part: i64 = 0;
        let mut frac_digits: u32 = 0;
        if self.cursor.peek() == Some(b'.') {
            has_dot = true;
            self.cursor.advance(1);
            while let Some(b) = self.cursor.peek() {
                if b.is_ascii_digit() {
                    frac_part = frac_part * 10 + (b - b'0') as i64;
                    frac_digits += 1;
                    self.cursor.advance(1);
                } else {
                    break;
                }
            }
        }

        if !has_int && frac_digits == 0 {
            return Err(PdfError::TokenError {
                pos: start_pos,
                msg: "invalid number".into(),
            });
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

    fn parse_string(&mut self) -> Result<PSToken> {
        self.cursor.advance(1); // skip '('
        let mut result = Vec::with_capacity(32);
        let mut depth = 1;

        while depth > 0 {
            match self.cursor.advance_one() {
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
                Some(b'\\') => match self.cursor.advance_one() {
                    Some(b'n') => result.push(b'\n'),
                    Some(b'r') => result.push(b'\r'),
                    Some(b't') => result.push(b'\t'),
                    Some(b'b') => result.push(0x08),
                    Some(b'f') => result.push(0x0c),
                    Some(b'(') => result.push(b'('),
                    Some(b')') => result.push(b')'),
                    Some(b'\\') => result.push(b'\\'),
                    Some(b'\r') => {
                        if self.cursor.peek() == Some(b'\n') {
                            self.cursor.advance(1);
                        }
                    }
                    Some(b'\n') => {}
                    Some(c) if c.is_ascii_digit() && c < b'8' => {
                        let mut octal = (c - b'0') as u32;
                        for _ in 0..2 {
                            if let Some(d) = self.cursor.peek() {
                                if d.is_ascii_digit() && d < b'8' {
                                    self.cursor.advance(1);
                                    octal = octal * 8 + (d - b'0') as u32;
                                } else {
                                    break;
                                }
                            }
                        }
                        result.push((octal & 0xFF) as u8);
                    }
                    Some(c) => result.push(c),
                    None => return Err(PdfError::UnexpectedEof),
                },
                Some(c) => result.push(c),
                None => return Err(PdfError::UnexpectedEof),
            }
        }

        Ok(PSToken::String(result))
    }

    fn parse_hex_string(&mut self) -> Result<PSToken> {
        self.cursor.advance(1); // skip '<'
        let mut result = Vec::new();
        let mut pending: Option<u8> = None;

        loop {
            match self.cursor.peek() {
                Some(b'>') => {
                    self.cursor.advance(1);
                    break;
                }
                Some(c) if c.is_ascii_hexdigit() => {
                    self.cursor.advance(1);
                    let nibble = hex_value(c).unwrap_or(0);
                    if let Some(high) = pending {
                        result.push((high << 4) | nibble);
                        pending = None;
                    } else {
                        pending = Some(nibble);
                    }
                }
                Some(c) if is_whitespace(c) => {
                    self.cursor.advance(1);
                }
                Some(_) => break,
                None => return Err(PdfError::UnexpectedEof),
            }
        }

        if let Some(nibble) = pending {
            result.push(nibble);
        }

        Ok(PSToken::String(result))
    }

    fn parse_keyword(&mut self) -> Result<PSToken> {
        if !self.cursor.current_slice().is_empty() {
            let slice = self.cursor.current_slice();
            match slice.iter().position(|&b| is_keyword_end(b)) {
                Some(end_idx) => {
                    let token = token_from_bytes(&slice[..end_idx]);
                    self.cursor.advance(end_idx);
                    return Ok(token);
                }
                None => {
                    if let Some(next) = self.cursor.peek_at(slice.len()) {
                        let starts_number = matches!(next, b'0'..=b'9' | b'+' | b'-' | b'.');
                        if starts_number {
                            let token = token_from_bytes(slice);
                            let should_stop = match &token {
                                PSToken::Keyword(Keyword::Unknown(_)) => false,
                                PSToken::Keyword(Keyword::D) => !matches!(next, b'0' | b'1'),
                                PSToken::Keyword(_) => true,
                                PSToken::Bool(_) => true,
                                _ => false,
                            };
                            if should_stop {
                                self.cursor.advance(slice.len());
                                return Ok(token);
                            }
                        }
                    }
                }
            }
        }

        let mut bytes = Vec::with_capacity(8);
        while let Some(b) = self.cursor.peek() {
            if is_keyword_end(b) {
                break;
            }
            bytes.push(b);
            self.cursor.advance(1);
        }

        Ok(token_from_bytes(&bytes))
    }

    fn read_inline_data(&mut self, target: &[u8]) -> Vec<u8> {
        self.cursor.normalize();
        while matches!(self.cursor.peek(), Some(b) if is_whitespace(b)) {
            self.cursor.advance(1);
        }

        let mut data = Vec::new();
        while !self.cursor.at_end() {
            if self.cursor.match_bytes(target) {
                let after = self.cursor.peek_at(target.len());
                if after.is_none_or(is_whitespace) {
                    self.cursor.advance(target.len());
                    if matches!(self.cursor.peek(), Some(b) if is_whitespace(b)) {
                        self.cursor.advance(1);
                    }
                    while data.last() == Some(&b'\r') || data.last() == Some(&b'\n') {
                        data.pop();
                    }
                    return data;
                }
            }
            if let Some(b) = self.cursor.advance_one() {
                data.push(b);
            } else {
                break;
            }
        }

        while data.last() == Some(&b'\r') || data.last() == Some(&b'\n') {
            data.pop();
        }
        data
    }
}

fn token_from_bytes(bytes: &[u8]) -> PSToken {
    if bytes == b"true" {
        return PSToken::Bool(true);
    } else if bytes == b"false" {
        return PSToken::Bool(false);
    }
    PSToken::Keyword(Keyword::from_bytes(bytes))
}

/// Parser for PDF content streams.
///
/// Content streams contain a sequence of operators and operands.
/// Operands precede their operator. Special handling is needed
/// for inline images (BI/ID/EI sequence).
pub struct PDFContentParser {
    /// Current position in data
    pos: usize,
    /// Total length of all streams
    total_len: usize,
    /// Pending tokens (for buffering during inline image handling)
    pending: VecDeque<(usize, ContentToken)>,
    /// Current operand stack
    operand_stack: Vec<(usize, PSToken)>,
    /// Context stack for nested arrays/dicts/procs
    context_stack: Vec<Context>,
    /// Whether we're collecting inline image dictionary
    in_inline_dict: bool,
    /// Lexer reused for tokenization
    lexer: SegmentedContentLexer,
}

impl PDFContentParser {
    /// Create a new content parser from one or more content streams.
    pub fn new(streams: Vec<Vec<u8>>) -> Self {
        let segments: Vec<Bytes> = streams.into_iter().map(Bytes::from).collect();
        let lexer = SegmentedContentLexer::new(segments);
        let total_len = lexer.total_len();

        Self {
            pos: 0,
            total_len,
            pending: VecDeque::new(),
            operand_stack: Vec::new(),
            context_stack: Vec::new(),
            in_inline_dict: false,
            lexer,
        }
    }

    /// Get next token with its position.
    pub fn next_with_pos(&mut self) -> Option<(usize, ContentToken)> {
        // Return pending tokens first
        if let Some(tok) = self.pending.pop_front() {
            return Some(tok);
        }

        loop {
            // Parse next token from base parser
            let (rel_pos, token) = match self.lexer.next_token() {
                Some(Ok(t)) => t,
                Some(Err(_)) => {
                    // Skip bad token and continue
                    self.pos += 1;
                    self.lexer.set_pos(self.pos);
                    continue;
                }
                None => {
                    // No more tokens - flush remaining operands
                    if !self.operand_stack.is_empty() {
                        for (pos, op) in self.operand_stack.drain(..) {
                            self.pending.push_back((pos, ContentToken::Operand(op)));
                        }
                        if let Some(tok) = self.pending.pop_front() {
                            return Some(tok);
                        }
                    }
                    return None;
                }
            };

            let abs_pos = rel_pos;
            let prev_pos = self.pos;
            self.pos = self.lexer.tell();
            if self.pos <= prev_pos && prev_pos < self.total_len {
                self.pos = prev_pos + 1;
            }
            self.lexer.set_pos(self.pos);

            match &token {
                PSToken::Keyword(kw) => {
                    // Handle structure keywords
                    match kw {
                        Keyword::ArrayStart => {
                            self.context_stack.push(Context::Array(abs_pos, Vec::new()));
                            continue;
                        }
                        Keyword::ArrayEnd => {
                            if let Some(Context::Array(arr_pos, items)) = self.context_stack.pop() {
                                let array_token = PSToken::Array(items);
                                if self.context_stack.is_empty() {
                                    // Top-level array - push to operand stack
                                    self.operand_stack.push((arr_pos, array_token));
                                } else {
                                    // Nested - push to parent context
                                    self.push_to_context(array_token);
                                }
                            }
                            continue;
                        }
                        Keyword::DictStart => {
                            self.context_stack.push(Context::Dict(abs_pos, Vec::new()));
                            continue;
                        }
                        Keyword::DictEnd => {
                            if let Some(Context::Dict(dict_pos, items)) = self.context_stack.pop() {
                                let dict = Self::build_dict(items);
                                let dict_token = PSToken::Dict(dict);
                                if self.context_stack.is_empty() {
                                    self.operand_stack.push((dict_pos, dict_token));
                                } else {
                                    self.push_to_context(dict_token);
                                }
                            }
                            continue;
                        }
                        Keyword::BraceOpen => {
                            self.context_stack.push(Context::Proc(abs_pos, Vec::new()));
                            continue;
                        }
                        Keyword::BraceClose => {
                            if let Some(Context::Proc(proc_pos, items)) = self.context_stack.pop() {
                                let proc_token = PSToken::Array(items);
                                if self.context_stack.is_empty() {
                                    self.operand_stack.push((proc_pos, proc_token));
                                } else {
                                    self.push_to_context(proc_token);
                                }
                            }
                            continue;
                        }
                        Keyword::BI => {
                            self.in_inline_dict = true;
                            self.operand_stack.clear();
                            continue;
                        }
                        Keyword::ID if self.in_inline_dict => {
                            self.in_inline_dict = false;
                            let dict = self.build_inline_dict();
                            let eos = self.get_inline_eos(&dict);
                            let img_data = self.get_inline_data(&eos);
                            self.pos = self.lexer.tell();
                            return Some((
                                abs_pos,
                                ContentToken::InlineImage {
                                    dict,
                                    data: img_data,
                                },
                            ));
                        }
                        Keyword::EI => {
                            // Already handled by ID processing
                            continue;
                        }
                        _ => {} // Fall through to normal keyword handling
                    }

                    // If we're inside inline dict, treat keyword as operand
                    if self.in_inline_dict {
                        self.operand_stack.push((abs_pos, token));
                        continue;
                    }

                    // If we're inside array/dict/proc context, push keyword as operand
                    if !self.context_stack.is_empty() {
                        self.push_to_context(token);
                        continue;
                    }

                    // Regular operator - flush operand stack and return keyword
                    for (pos, op) in self.operand_stack.drain(..) {
                        self.pending.push_back((pos, ContentToken::Operand(op)));
                    }
                    self.pending
                        .push_back((abs_pos, ContentToken::Keyword(kw.clone())));

                    if let Some(tok) = self.pending.pop_front() {
                        return Some(tok);
                    }
                }
                _ => {
                    if self.in_inline_dict {
                        self.operand_stack.push((abs_pos, token));
                    } else if !self.context_stack.is_empty() {
                        self.push_to_context(token);
                    } else {
                        self.operand_stack.push((abs_pos, token));
                    }
                    continue;
                }
            }
        }
    }

    /// Push a token to the current context (array/dict/proc)
    fn push_to_context(&mut self, token: PSToken) {
        if let Some(ctx) = self.context_stack.last_mut() {
            match ctx {
                Context::Array(_, items) => items.push(token),
                Context::Dict(_, items) => items.push(token),
                Context::Proc(_, items) => items.push(token),
            }
        }
    }

    /// Build dictionary from key-value pairs
    fn build_dict(items: Vec<PSToken>) -> HashMap<String, PSToken> {
        let mut dict = HashMap::new();
        let mut iter = items.into_iter();
        while let Some(key) = iter.next() {
            if let PSToken::Literal(name) = key
                && let Some(value) = iter.next()
            {
                dict.insert(name, value);
            }
        }
        dict
    }

    /// Build dictionary from collected operands (for inline images)
    fn build_inline_dict(&self) -> HashMap<String, PSToken> {
        let mut dict = HashMap::new();
        let mut iter = self.operand_stack.iter();
        while let Some((_, key)) = iter.next() {
            if let PSToken::Literal(name) = key
                && let Some((_, value)) = iter.next()
            {
                dict.insert(name.clone(), value.clone());
            }
        }
        dict
    }

    /// Determine end-of-stream marker for inline image.
    fn get_inline_eos(&self, dict: &HashMap<String, PSToken>) -> Vec<u8> {
        let filter = dict.get("F").or_else(|| dict.get("Filter"));

        if let Some(PSToken::Literal(name)) = filter
            && (name == "A85" || name == "ASCII85Decode")
        {
            return b"~>".to_vec();
        }

        if let Some(PSToken::Array(filters)) = filter
            && let Some(PSToken::Literal(name)) = filters.first()
            && (name == "A85" || name == "ASCII85Decode")
        {
            return b"~>".to_vec();
        }

        b"EI".to_vec()
    }

    /// Get inline image data by scanning for end marker.
    fn get_inline_data(&mut self, target: &[u8]) -> Vec<u8> {
        self.lexer.read_inline_data(target)
    }
}

impl Iterator for PDFContentParser {
    type Item = ContentToken;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_with_pos().map(|(_, token)| token)
    }
}

/// Check if byte is PDF whitespace.
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
    is_whitespace(b) || is_delimiter(b)
}

const fn hex_value(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

// ============================================================================
// PDFResourceManager
// ============================================================================

/// Unique identifier for a cached font.
pub type FontId = u64;

/// Repository of shared resources.
///
/// ResourceManager facilitates reuse of shared resources such as fonts
/// and images so that large objects are not allocated multiple times.
///
/// Port of pdfminer.six PDFResourceManager class.
pub struct PDFResourceManager {
    /// Whether caching is enabled
    caching: bool,
    /// Cached fonts: objid -> FontId
    cached_fonts: HashMap<u64, FontId>,
    /// Counter for generating unique font IDs
    next_font_id: FontId,
}

impl PDFResourceManager {
    /// Create a new PDFResourceManager with caching enabled.
    pub fn new() -> Self {
        Self::with_caching(true)
    }

    /// Create a new PDFResourceManager with specified caching behavior.
    pub fn with_caching(caching: bool) -> Self {
        Self {
            caching,
            cached_fonts: HashMap::new(),
            next_font_id: 1,
        }
    }

    /// Check if caching is enabled.
    pub const fn caching_enabled(&self) -> bool {
        self.caching
    }

    /// Process a ProcSet array.
    ///
    /// In PDF, ProcSet defines which procedure sets are needed.
    /// This is largely obsolete and we just log/ignore like Python does.
    pub const fn get_procset(&self, _procs: &[&str]) {
        // Matches Python behavior: essentially a no-op
        // Python iterates procs and checks for LITERAL_PDF/LITERAL_TEXT
        // but doesn't do anything meaningful with them
    }

    /// Get a predefined color space by name.
    ///
    /// Returns None if the color space is not in the predefined set.
    pub fn get_colorspace(&self, name: &str) -> Option<PDFColorSpace> {
        PREDEFINED_COLORSPACE.get(name).cloned()
    }

    /// Get or create a font from the specification.
    ///
    /// If objid is provided and caching is enabled, returns cached font
    /// if already loaded. Otherwise creates a new font entry.
    ///
    /// Returns a FontId that can be used to reference the font.
    pub fn get_font(&mut self, objid: Option<u64>, _spec: &HashMap<String, PDFObject>) -> FontId {
        // Check cache if objid provided and caching enabled
        if let Some(id) = objid
            && self.caching
            && let Some(&font_id) = self.cached_fonts.get(&id)
        {
            return font_id;
        }

        // Create new font entry
        let font_id = self.next_font_id;
        self.next_font_id += 1;

        // Cache if objid provided and caching enabled
        if let Some(id) = objid
            && self.caching
        {
            self.cached_fonts.insert(id, font_id);
        }

        font_id
    }

    /// Get a CMap by name.
    ///
    /// If strict is true and the CMap is not found, returns an error.
    /// If strict is false and the CMap is not found, returns an empty CMap.
    ///
    /// Currently only handles Identity CMaps (Identity-H, Identity-V,
    /// DLIdent-H, DLIdent-V). Other CMaps will be loaded from embedded
    /// data in a future implementation.
    pub fn get_cmap(&self, cmapname: &str, strict: bool) -> Result<CMap> {
        // Check for identity CMaps first
        if CMapDB::is_identity_cmap(cmapname) || CMapDB::is_identity_cmap_byte(cmapname) {
            // For identity CMaps, return an empty CMap with appropriate vertical mode
            // The actual identity mapping is handled by IdentityCMap/IdentityCMapByte types
            let mut cmap = CMap::new();
            cmap.set_vertical(CMapDB::is_vertical(cmapname));
            cmap.attrs
                .insert("CMapName".to_string(), cmapname.to_string());
            return Ok(cmap);
        }

        // CMap not found - either error or return empty CMap
        if strict {
            Err(PdfError::CMapNotFound(cmapname.to_string()))
        } else {
            Ok(CMap::new())
        }
    }
}

impl Default for PDFResourceManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// PDFPageInterpreter
// ============================================================================

use super::device::{PDFDevice, PDFStackT, PDFStackValue, PDFTextSeq, PDFTextSeqItem, PathSegment};
use crate::pdfstate::{PDFGraphicState, PDFTextState};
use crate::utils::{MATRIX_IDENTITY, Matrix, mult_matrix};

/// Saved graphics state for q/Q operators.
type SavedState = (Matrix, PDFTextState, PDFGraphicState);

/// PDF Page Interpreter - executes PDF content stream operators.
///
/// Port of PDFPageInterpreter from pdfminer.six pdfinterp.py
///
/// Reference: PDF Reference, Appendix A, Operator Summary
///
/// Note: Method names like `do_Q`, `do_S`, `do_B` intentionally use uppercase
/// to match PDF operator names from the spec (q/Q, s/S, b/B, etc.).
pub struct PDFPageInterpreter<'a, D: PDFDevice> {
    /// Resource manager for fonts, color spaces, etc.
    #[allow(dead_code)]
    pub(crate) rsrcmgr: &'a mut PDFResourceManager,
    /// Output device for rendering operations
    pub(crate) device: &'a mut D,
    /// Graphics state stack for q/Q operators
    pub(crate) gstack: Vec<SavedState>,
    /// Current transformation matrix
    pub(crate) ctm: Matrix,
    /// Current text state
    pub(crate) textstate: PDFTextState,
    /// Current graphics state
    pub(crate) graphicstate: PDFGraphicState,
    /// Current path being constructed
    pub(crate) curpath: Vec<PathSegment>,
    /// Current point for path operations (used by v operator)
    pub(crate) current_point: Option<(f64, f64)>,
    /// Font map: font name -> PDFCIDFont
    pub(crate) fontmap: HashMap<String, std::sync::Arc<crate::pdffont::PDFCIDFont>>,
    /// Current resources dictionary (for XObject lookup fallback)
    pub(crate) resources: HashMap<String, PDFObject>,
    /// XObject map: name -> stream
    pub(crate) xobjmap: HashMap<String, PDFStream>,
    /// Inline image counter
    pub(crate) inline_image_id: usize,
    /// Stack of active form XObjects to prevent recursion
    pub(crate) xobj_stack: Vec<String>,
    /// Document reference for resolving XObject resources
    pub(crate) doc: Option<&'a crate::pdfdocument::PDFDocument>,
}

#[allow(non_snake_case)]
impl<'a, D: PDFDevice> PDFPageInterpreter<'a, D> {
    /// Create a new PDFPageInterpreter.
    pub fn new(rsrcmgr: &'a mut PDFResourceManager, device: &'a mut D) -> Self {
        Self {
            rsrcmgr,
            device,
            gstack: Vec::new(),
            ctm: MATRIX_IDENTITY,
            textstate: PDFTextState::new(),
            graphicstate: PDFGraphicState::new(),
            curpath: Vec::new(),
            current_point: None,
            fontmap: HashMap::new(),
            resources: HashMap::new(),
            xobjmap: HashMap::new(),
            inline_image_id: 0,
            xobj_stack: Vec::new(),
            doc: None,
        }
    }

    /// Initialize graphics state for rendering.
    ///
    /// Called at the start of page rendering.
    pub fn init_state(&mut self, ctm: Matrix) {
        self.gstack.clear();
        self.ctm = ctm;
        self.device.set_ctm(self.ctm);
        self.textstate = PDFTextState::new();
        self.graphicstate = PDFGraphicState::new();
        self.curpath.clear();
        self.current_point = None;
    }

    /// Get current transformation matrix.
    pub const fn ctm(&self) -> Matrix {
        self.ctm
    }

    /// Get current graphics state (read-only).
    pub const fn graphicstate(&self) -> &PDFGraphicState {
        &self.graphicstate
    }

    /// Get current text state (read-only).
    pub const fn textstate(&self) -> &PDFTextState {
        &self.textstate
    }

    /// Get current text state (mutable).
    pub const fn textstate_mut(&mut self) -> &mut PDFTextState {
        &mut self.textstate
    }

    /// Get current path (read-only).
    pub fn current_path(&self) -> &[PathSegment] {
        &self.curpath
    }

    /// Initialize resources from a page's resource dictionary.
    ///
    /// Builds the fontmap from Font resources, parsing ToUnicode streams.
    ///
    /// Port of PDFPageInterpreter.init_resources from pdfminer.six
    pub fn init_resources(
        &mut self,
        resources: &HashMap<String, PDFObject>,
        doc: Option<&'a crate::pdfdocument::PDFDocument>,
    ) {
        self.fontmap.clear();
        self.xobjmap.clear();
        self.resources = resources.clone();

        // Get Font dictionary (optional)
        let fonts = match resources.get("Font") {
            Some(PDFObject::Dict(d)) => Some(d.clone()),
            Some(PDFObject::Ref(r)) => {
                if let Some(doc) = doc {
                    match doc.resolve_shared(&PDFObject::Ref(r.clone())) {
                        Ok(resolved) => match resolved.as_ref() {
                            PDFObject::Dict(d) => Some(d.clone()),
                            _ => None,
                        },
                        Err(_) => None,
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        // Process each font
        if let Some(fonts) = fonts {
            for (fontid, spec_obj) in fonts.iter() {
                let spec = match spec_obj {
                    PDFObject::Dict(d) => d.clone(),
                    PDFObject::Ref(r) => {
                        if let Some(doc) = doc {
                            if let Ok(resolved) = doc.resolve_shared(&PDFObject::Ref(r.clone())) {
                                if let PDFObject::Dict(d) = resolved.as_ref() {
                                    d.clone()
                                } else {
                                    continue;
                                }
                            } else {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    }
                    _ => continue,
                };

                // Get font subtype
                let subtype = spec
                    .get("Subtype")
                    .and_then(|s| s.as_name().ok())
                    .unwrap_or("")
                    .to_string();

                // Handle Type0 fonts - merge ToUnicode and Encoding into descendant font spec
                let (final_spec, tounicode_data) = if subtype == "Type0" {
                    // Get descendant font spec
                    let descendant_spec = Self::get_descendant_font_spec(&spec, doc);
                    if let Some(mut dspec) = descendant_spec {
                        // Copy ToUnicode and Encoding from Type0 to descendant
                        if let Some(v) = spec.get("ToUnicode") {
                            dspec.insert("ToUnicode".to_string(), v.clone());
                        }
                        if let Some(v) = spec.get("Encoding") {
                            dspec.insert("Encoding".to_string(), v.clone());
                        }
                        let tounicode = Self::extract_tounicode(&dspec, doc);
                        (dspec, tounicode)
                    } else {
                        let tounicode = Self::extract_tounicode(&spec, doc);
                        (spec, tounicode)
                    }
                } else {
                    let tounicode = Self::extract_tounicode(&spec, doc);
                    (spec, tounicode)
                };

                // Resolve Encoding reference if present (needed for Type1 fonts with custom encodings)
                let mut final_spec = final_spec;
                let mut cached_encoding: Option<Arc<HashMap<u8, String>>> = None;
                if let Some(PDFObject::Ref(r)) = final_spec.get("Encoding").cloned()
                    && let Some(doc) = doc
                {
                    let objid = r.objid;
                    if let Ok(resolved) = doc.resolve_shared(&PDFObject::Ref(r)) {
                        cached_encoding = doc.get_or_build_font_encoding(objid, resolved.as_ref());
                        final_spec.insert("Encoding".to_string(), resolved.as_ref().clone());
                    }
                }

                // Resolve Widths reference if present (needed for simple fonts)
                if let Some(PDFObject::Ref(r)) = final_spec.get("Widths").cloned()
                    && let Some(doc) = doc
                    && let Ok(resolved) = doc.resolve_shared(&PDFObject::Ref(r))
                {
                    final_spec.insert("Widths".to_string(), resolved.as_ref().clone());
                }

                // Resolve W (CID font widths) reference if present
                if let Some(PDFObject::Ref(r)) = final_spec.get("W").cloned()
                    && let Some(doc) = doc
                    && let Ok(resolved) = doc.resolve_shared(&PDFObject::Ref(r))
                {
                    final_spec.insert("W".to_string(), resolved.as_ref().clone());
                }

                // Resolve FontDescriptor reference if present (needed for accurate ascent/descent)
                if let Some(PDFObject::Ref(r)) = final_spec.get("FontDescriptor").cloned()
                    && let Some(doc) = doc
                    && let Ok(resolved) = doc.resolve_shared(&PDFObject::Ref(r))
                {
                    final_spec.insert("FontDescriptor".to_string(), resolved.as_ref().clone());
                }

                // Extract FontFile2 (TrueType font data) if available
                let ttf_data = Self::extract_fontfile2(&final_spec, doc);

                // Create font with ToUnicode data and TrueType font data
                let font = crate::pdffont::PDFCIDFont::new_with_ttf_and_cid2unicode(
                    &final_spec,
                    tounicode_data.as_deref(),
                    ttf_data.as_deref(),
                    subtype == "Type0",
                    Some(fontid.clone()),
                    cached_encoding,
                );
                self.fontmap
                    .insert(fontid.clone(), std::sync::Arc::new(font));
            }
        }

        // Build XObject map (optional)
        let xobjects = match resources.get("XObject") {
            Some(PDFObject::Dict(d)) => Some(d.clone()),
            Some(PDFObject::Ref(r)) => {
                if let Some(doc) = doc {
                    match doc.resolve_shared(&PDFObject::Ref(r.clone())) {
                        Ok(resolved) => match resolved.as_ref() {
                            PDFObject::Dict(d) => Some(d.clone()),
                            _ => None,
                        },
                        Err(_) => None,
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some(xobjects) = xobjects {
            for (xobjid, xobj) in xobjects.iter() {
                let stream = match xobj {
                    PDFObject::Stream(s) => Some((**s).clone()),
                    PDFObject::Ref(r) => {
                        if let Some(doc) = doc {
                            match doc.resolve_shared(&PDFObject::Ref(r.clone())) {
                                Ok(resolved) => match resolved.as_ref() {
                                    PDFObject::Stream(s) => Some((**s).clone()),
                                    _ => None,
                                },
                                Err(_) => None,
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if let Some(mut stream) = stream {
                    if let Some(doc) = doc {
                        Self::resolve_jbig2_globals(&mut stream, doc);
                    }
                    self.xobjmap.insert(xobjid.clone(), stream);
                }
            }
        }
    }

    /// Get the first descendant font spec from a Type0 font.
    fn get_descendant_font_spec(
        spec: &HashMap<String, PDFObject>,
        doc: Option<&crate::pdfdocument::PDFDocument>,
    ) -> Option<HashMap<String, PDFObject>> {
        let dfonts = spec.get("DescendantFonts")?;

        // Resolve if reference
        let dfonts_resolved = match dfonts {
            PDFObject::Array(arr) => arr.clone(),
            PDFObject::Ref(r) => {
                if let Some(doc) = doc {
                    if let Ok(resolved) = doc.resolve_shared(&PDFObject::Ref(r.clone())) {
                        if let PDFObject::Array(arr) = resolved.as_ref() {
                            arr.clone()
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }
            _ => return None,
        };

        // Get first descendant font
        let first = dfonts_resolved.first()?;

        // Resolve font spec
        match first {
            PDFObject::Dict(d) => Some(d.clone()),
            PDFObject::Ref(r) => {
                if let Some(doc) = doc {
                    if let Ok(resolved) = doc.resolve_shared(&PDFObject::Ref(r.clone())) {
                        if let PDFObject::Dict(d) = resolved.as_ref() {
                            Some(d.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn stream_has_jbig2_filter(stream: &PDFStream) -> bool {
        match stream.get("Filter") {
            Some(PDFObject::Name(name)) => name.eq_ignore_ascii_case("JBIG2Decode"),
            Some(PDFObject::Array(arr)) => arr.iter().any(|obj| {
                matches!(obj, PDFObject::Name(name) if name.eq_ignore_ascii_case("JBIG2Decode"))
            }),
            _ => false,
        }
    }

    fn resolve_decode_parms(obj: PDFObject, doc: &crate::pdfdocument::PDFDocument) -> PDFObject {
        match obj {
            PDFObject::Ref(r) => match doc.resolve(&PDFObject::Ref(r.clone())) {
                Ok(resolved) => Self::resolve_decode_parms(resolved, doc),
                Err(_) => PDFObject::Ref(r),
            },
            PDFObject::Dict(mut dict) => {
                if let Some(jbig2) = dict.get("JBIG2Globals").cloned() {
                    let resolved = match jbig2 {
                        PDFObject::Ref(r) => doc
                            .resolve(&PDFObject::Ref(r.clone()))
                            .unwrap_or(PDFObject::Ref(r)),
                        other => other,
                    };
                    dict.insert("JBIG2Globals".to_string(), resolved);
                }
                PDFObject::Dict(dict)
            }
            PDFObject::Array(arr) => PDFObject::Array(
                arr.into_iter()
                    .map(|item| Self::resolve_decode_parms(item, doc))
                    .collect(),
            ),
            other => other,
        }
    }

    fn resolve_jbig2_globals(stream: &mut PDFStream, doc: &crate::pdfdocument::PDFDocument) {
        if !Self::stream_has_jbig2_filter(stream) {
            return;
        }

        if let Some(params) = stream.get("DecodeParms").cloned() {
            let resolved = Self::resolve_decode_parms(params, doc);
            stream.attrs.insert("DecodeParms".to_string(), resolved);
        }
    }

    /// Extract ToUnicode stream data from font spec.
    fn extract_tounicode(
        spec: &HashMap<String, PDFObject>,
        doc: Option<&crate::pdfdocument::PDFDocument>,
    ) -> Option<Vec<u8>> {
        let tounicode = spec.get("ToUnicode")?;

        match tounicode {
            PDFObject::Stream(stream) => {
                // Decode the stream directly
                if let Some(doc) = doc {
                    doc.decode_stream(stream).ok()
                } else {
                    Some(stream.get_data().to_vec())
                }
            }
            PDFObject::Ref(r) => {
                // Resolve reference and decode
                if let Some(doc) = doc {
                    if let Ok(resolved) = doc.resolve_shared(&PDFObject::Ref(r.clone())) {
                        if let PDFObject::Stream(stream) = resolved.as_ref() {
                            doc.decode_stream(stream).ok()
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Extract FontFile2 (TrueType font) data from font spec.
    ///
    /// Follows the chain: spec["FontDescriptor"]["FontFile2"]
    fn extract_fontfile2(
        spec: &HashMap<String, PDFObject>,
        doc: Option<&crate::pdfdocument::PDFDocument>,
    ) -> Option<Vec<u8>> {
        // Get FontDescriptor
        let font_descriptor = spec.get("FontDescriptor")?;

        // Resolve FontDescriptor if it's a reference
        let fd_dict = match font_descriptor {
            PDFObject::Dict(d) => d.clone(),
            PDFObject::Ref(r) => {
                if let Some(doc) = doc {
                    if let Ok(resolved) = doc.resolve_shared(&PDFObject::Ref(r.clone())) {
                        if let PDFObject::Dict(d) = resolved.as_ref() {
                            d.clone()
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }
            _ => return None,
        };

        // Get FontFile2
        let fontfile2 = fd_dict.get("FontFile2")?;

        // Resolve and decode the stream
        match fontfile2 {
            PDFObject::Stream(stream) => {
                if let Some(doc) = doc {
                    doc.decode_stream(stream).ok()
                } else {
                    Some(stream.get_data().to_vec())
                }
            }
            PDFObject::Ref(r) => {
                if let Some(doc) = doc {
                    if let Ok(resolved) = doc.resolve_shared(&PDFObject::Ref(r.clone())) {
                        if let PDFObject::Stream(stream) = resolved.as_ref() {
                            doc.decode_stream(stream).ok()
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Get current state tuple for saving.
    pub(crate) fn get_current_state(&self) -> SavedState {
        (self.ctm, self.textstate.copy(), self.graphicstate.copy())
    }

    /// Set current state from saved tuple.
    pub(crate) fn set_current_state(&mut self, state: SavedState) {
        let (ctm, textstate, graphicstate) = state;
        self.ctm = ctm;
        self.textstate = textstate;
        self.graphicstate = graphicstate;
        self.device.set_ctm(self.ctm);
    }

    // ========================================================================
    // Page Processing
    // ========================================================================

    /// Process a PDF page.
    ///
    /// This is the main entry point for page interpretation.
    /// Sets up the CTM based on page rotation, then renders the content streams.
    ///
    /// Port of PDFPageInterpreter.process_page from pdfminer.six
    pub fn process_page(
        &mut self,
        page: &crate::pdfpage::PDFPage,
        doc: Option<&'a crate::pdfdocument::PDFDocument>,
    ) {
        self.doc = doc;
        let mediabox = page.mediabox.unwrap_or([0.0, 0.0, 612.0, 792.0]);
        let (x0, y0, x1, y1) = (mediabox[0], mediabox[1], mediabox[2], mediabox[3]);

        // Calculate CTM based on page rotation
        let mut ctm = match page.rotate {
            90 => (0.0, -1.0, 1.0, 0.0, -y0, x1),
            180 => (-1.0, 0.0, 0.0, -1.0, x1, y1),
            270 => (0.0, 1.0, -1.0, 0.0, y1, -x0),
            _ => (1.0, 0.0, 0.0, 1.0, -x0, -y0),
        };

        // Apply UserUnit scaling (PDF 1.6 feature)
        let user_unit = page.user_unit;
        if user_unit != 1.0 {
            ctm = mult_matrix((user_unit, 0.0, 0.0, user_unit, 0.0, 0.0), ctm);
        }

        // Begin page on device
        let bbox = (x0, y0, x1, y1);
        self.device.begin_page(page.pageid, bbox, ctm);

        // Initialize resources (builds fontmap)
        self.init_resources(&page.resources, doc);

        // Initialize state and execute content streams
        self.init_state(ctm);
        let streams = if page.contents.is_empty() {
            doc.map(|doc| crate::pdfpage::PDFPage::parse_contents(&page.attrs, doc))
                .unwrap_or_default()
        } else {
            page.contents.clone()
        };
        self.execute(&streams);

        // End page on device
        self.device.end_page(page.pageid);
    }

    /// Execute content streams.
    ///
    /// Parses the content streams and dispatches operators to do_* methods.
    ///
    /// Port of PDFPageInterpreter.execute from pdfminer.six
    pub fn execute(&mut self, streams: &[Vec<u8>]) {
        if streams.is_empty() {
            return;
        }

        let parser = PDFContentParser::new(streams.to_vec());
        let mut operand_stack: Vec<PSToken> = Vec::new();

        for token in parser {
            match token {
                ContentToken::Operand(op) => {
                    operand_stack.push(op);
                }
                ContentToken::Keyword(name) => {
                    self.dispatch_operator(&name, &mut operand_stack);
                    operand_stack.clear();
                }
                ContentToken::InlineImage { dict, data } => {
                    let mut attrs = HashMap::new();
                    for (key, value) in dict {
                        let obj = match value {
                            PSToken::Int(n) => PDFObject::Int(n),
                            PSToken::Real(n) => PDFObject::Real(n),
                            PSToken::Bool(b) => PDFObject::Bool(b),
                            PSToken::Literal(name) => PDFObject::Name(name),
                            PSToken::String(s) => PDFObject::String(s),
                            PSToken::Array(arr) => {
                                let mut vals = Vec::new();
                                for item in arr {
                                    match item {
                                        PSToken::Int(n) => vals.push(PDFObject::Int(n)),
                                        PSToken::Real(n) => vals.push(PDFObject::Real(n)),
                                        PSToken::Bool(b) => vals.push(PDFObject::Bool(b)),
                                        PSToken::Literal(name) => vals.push(PDFObject::Name(name)),
                                        PSToken::String(s) => vals.push(PDFObject::String(s)),
                                        _ => {}
                                    }
                                }
                                PDFObject::Array(vals)
                            }
                            PSToken::Dict(d) => {
                                let mut map = HashMap::new();
                                for (k, v) in d {
                                    let vobj = match v {
                                        PSToken::Int(n) => PDFObject::Int(n),
                                        PSToken::Real(n) => PDFObject::Real(n),
                                        PSToken::Bool(b) => PDFObject::Bool(b),
                                        PSToken::Literal(name) => PDFObject::Name(name),
                                        PSToken::String(s) => PDFObject::String(s),
                                        _ => PDFObject::Null,
                                    };
                                    map.insert(k, vobj);
                                }
                                PDFObject::Dict(map)
                            }
                            _ => PDFObject::Null,
                        };
                        let key = match key.as_str() {
                            "BPC" => "BitsPerComponent",
                            "CS" => "ColorSpace",
                            "W" => "Width",
                            "H" => "Height",
                            "IM" => "ImageMask",
                            "DP" => "DecodeParms",
                            "F" => "Filter",
                            _ => key.as_str(),
                        }
                        .to_string();
                        attrs.insert(key, obj);
                    }
                    let stream = PDFStream::new(attrs, data);
                    let name = format!("inline{}", self.inline_image_id);
                    self.inline_image_id += 1;
                    self.device
                        .begin_figure(&name, (0.0, 0.0, 1.0, 1.0), MATRIX_IDENTITY);
                    self.device.render_image(&name, &stream);
                    self.device.end_figure(&name);
                    operand_stack.clear();
                }
            }
        }
    }

    fn pstoken_to_stackvalue(token: &PSToken) -> Option<PDFStackValue> {
        match token {
            PSToken::Int(n) => Some(PDFStackValue::Int(*n)),
            PSToken::Real(n) => Some(PDFStackValue::Real(*n)),
            PSToken::Bool(b) => Some(PDFStackValue::Bool(*b)),
            PSToken::Literal(name) => Some(PDFStackValue::Name(name.clone())),
            PSToken::String(s) => Some(PDFStackValue::String(s.clone())),
            PSToken::Array(arr) => {
                let values = arr.iter().filter_map(Self::pstoken_to_stackvalue).collect();
                Some(PDFStackValue::Array(values))
            }
            PSToken::Dict(map) => {
                let mut values = HashMap::new();
                for (key, val) in map.iter() {
                    if let Some(v) = Self::pstoken_to_stackvalue(val) {
                        values.insert(key.clone(), v);
                    }
                }
                Some(PDFStackValue::Dict(values))
            }
            PSToken::Keyword(_) => None,
        }
    }

    fn pdfobject_to_stackvalue(&self, obj: &PDFObject) -> Option<PDFStackValue> {
        match obj {
            PDFObject::Int(n) => Some(PDFStackValue::Int(*n)),
            PDFObject::Real(n) => Some(PDFStackValue::Real(*n)),
            PDFObject::Bool(b) => Some(PDFStackValue::Bool(*b)),
            PDFObject::Name(name) => Some(PDFStackValue::Name(name.clone())),
            PDFObject::String(s) => Some(PDFStackValue::String(s.clone())),
            PDFObject::Array(arr) => {
                let values = arr
                    .iter()
                    .filter_map(|item| self.pdfobject_to_stackvalue(item))
                    .collect();
                Some(PDFStackValue::Array(values))
            }
            PDFObject::Dict(map) => {
                let mut values = HashMap::new();
                for (key, val) in map.iter() {
                    if let Some(v) = self.pdfobject_to_stackvalue(val) {
                        values.insert(key.clone(), v);
                    }
                }
                Some(PDFStackValue::Dict(values))
            }
            PDFObject::Ref(r) => {
                if let Some(doc) = self.doc
                    && let Ok(resolved) = doc.resolve_shared(&PDFObject::Ref(r.clone()))
                {
                    return self.pdfobject_to_stackvalue(resolved.as_ref());
                }
                None
            }
            _ => None,
        }
    }

    fn properties_dict(&self) -> Option<HashMap<String, PDFObject>> {
        match self.resources.get("Properties") {
            Some(PDFObject::Dict(d)) => Some(d.clone()),
            Some(PDFObject::Ref(r)) => {
                if let Some(doc) = self.doc {
                    match doc.resolve_shared(&PDFObject::Ref(r.clone())) {
                        Ok(resolved) => match resolved.as_ref() {
                            PDFObject::Dict(d) => Some(d.clone()),
                            _ => None,
                        },
                        Err(_) => None,
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn props_from_token(&self, token: Option<PSToken>) -> PDFStackT {
        let mut props = HashMap::new();
        match token {
            Some(PSToken::Dict(map)) => {
                for (key, val) in map {
                    if let Some(v) = Self::pstoken_to_stackvalue(&val) {
                        props.insert(key, v);
                    }
                }
            }
            Some(PSToken::Literal(name)) => {
                if let Some(dict) = self.properties_dict()
                    && let Some(obj) = dict.get(&name)
                    && let Some(PDFStackValue::Dict(map)) = self.pdfobject_to_stackvalue(obj)
                {
                    props = map;
                }
            }
            _ => {}
        }
        props
    }

    /// Dispatch an operator to the appropriate do_* method.
    fn dispatch_operator(&mut self, op: &Keyword, args: &mut Vec<PSToken>) {
        match op {
            // Graphics state operators
            Keyword::Qq => self.do_q(),
            Keyword::Q => self.do_Q(),
            Keyword::Cm => {
                if let Some((a, b, c, d, e, f)) = Self::pop_matrix(args) {
                    self.do_cm(a, b, c, d, e, f);
                }
            }
            Keyword::Ww => {
                if let Some(w) = Self::pop_number(args) {
                    self.do_w(w);
                }
            }
            Keyword::J => {
                if let Some(n) = Self::pop_int(args) {
                    self.do_J(n);
                }
            }
            Keyword::Jj => {
                if let Some(n) = Self::pop_int(args) {
                    self.do_j(n);
                }
            }
            Keyword::M => {
                if let Some(m) = Self::pop_number(args) {
                    self.do_M(m);
                }
            }
            Keyword::D => {
                // dash pattern: [array] phase
                if args.len() >= 2 {
                    let phase = Self::pop_number(args).unwrap_or(0.0);
                    let arr = Self::pop_array(args).unwrap_or_default();
                    self.do_d(arr, phase);
                }
            }
            Keyword::Ri => {
                if let Some(intent) = Self::pop_name(args) {
                    self.do_ri(&intent);
                }
            }
            Keyword::I => {
                if let Some(f) = Self::pop_number(args) {
                    self.do_i(f);
                }
            }
            Keyword::Gs => {
                if let Some(name) = Self::pop_name(args) {
                    self.do_gs(&name);
                }
            }
            Keyword::Do => {
                if let Some(name) = Self::pop_name(args) {
                    self.do_Do(name);
                }
            }

            // Marked content operators
            Keyword::BMC => {
                if let Some(name) = Self::pop_name(args) {
                    let tag = PSLiteral::new(&name);
                    self.do_BMC(&tag);
                }
            }
            Keyword::BDC => {
                // BDC takes tag and properties dict
                let props_token = args.pop();
                if let Some(name) = Self::pop_name(args) {
                    let tag = PSLiteral::new(&name);
                    let props_map = self.props_from_token(props_token);
                    self.do_BDC(&tag, &props_map);
                }
            }
            Keyword::EMC => {
                self.do_EMC();
            }

            // Path construction operators
            Keyword::Mm => {
                if let Some((x, y)) = Self::pop_point(args) {
                    self.do_m(x, y);
                }
            }
            Keyword::L => {
                if let Some((x, y)) = Self::pop_point(args) {
                    self.do_l(x, y);
                }
            }
            Keyword::C => {
                if args.len() >= 6 {
                    let y3 = Self::pop_number(args).unwrap_or(0.0);
                    let x3 = Self::pop_number(args).unwrap_or(0.0);
                    let y2 = Self::pop_number(args).unwrap_or(0.0);
                    let x2 = Self::pop_number(args).unwrap_or(0.0);
                    let y1 = Self::pop_number(args).unwrap_or(0.0);
                    let x1 = Self::pop_number(args).unwrap_or(0.0);
                    self.do_c(x1, y1, x2, y2, x3, y3);
                }
            }
            Keyword::V => {
                if args.len() >= 4 {
                    let y3 = Self::pop_number(args).unwrap_or(0.0);
                    let x3 = Self::pop_number(args).unwrap_or(0.0);
                    let y2 = Self::pop_number(args).unwrap_or(0.0);
                    let x2 = Self::pop_number(args).unwrap_or(0.0);
                    self.do_v(x2, y2, x3, y3);
                }
            }
            Keyword::Y => {
                if args.len() >= 4 {
                    let y3 = Self::pop_number(args).unwrap_or(0.0);
                    let x3 = Self::pop_number(args).unwrap_or(0.0);
                    let y1 = Self::pop_number(args).unwrap_or(0.0);
                    let x1 = Self::pop_number(args).unwrap_or(0.0);
                    self.do_y(x1, y1, x3, y3);
                }
            }
            Keyword::H => self.do_h(),
            Keyword::Re => {
                if args.len() >= 4 {
                    let h = Self::pop_number(args).unwrap_or(0.0);
                    let w = Self::pop_number(args).unwrap_or(0.0);
                    let y = Self::pop_number(args).unwrap_or(0.0);
                    let x = Self::pop_number(args).unwrap_or(0.0);
                    self.do_re(x, y, w, h);
                }
            }

            // Path painting operators
            Keyword::S => self.do_S(),
            Keyword::Ss => self.do_s(),
            Keyword::Ff | Keyword::F => self.do_f(),
            Keyword::FStar => self.do_f_star(),
            Keyword::B => self.do_B(),
            Keyword::BStar => self.do_B_star(),
            Keyword::Bb => self.do_b(),
            Keyword::BbStar => self.do_b_star(),
            Keyword::N => self.do_n(),

            // Color operators
            Keyword::G => {
                if let Some(g) = Self::pop_number(args) {
                    self.do_G(g);
                }
            }
            Keyword::Gg => {
                if let Some(g) = Self::pop_number(args) {
                    self.do_g(g);
                }
            }
            Keyword::RG => {
                if args.len() >= 3 {
                    let b = Self::pop_number(args).unwrap_or(0.0);
                    let g = Self::pop_number(args).unwrap_or(0.0);
                    let r = Self::pop_number(args).unwrap_or(0.0);
                    self.do_RG(r, g, b);
                }
            }
            Keyword::Rg => {
                if args.len() >= 3 {
                    let b = Self::pop_number(args).unwrap_or(0.0);
                    let g = Self::pop_number(args).unwrap_or(0.0);
                    let r = Self::pop_number(args).unwrap_or(0.0);
                    self.do_rg(r, g, b);
                }
            }
            Keyword::K => {
                if args.len() >= 4 {
                    let k = Self::pop_number(args).unwrap_or(0.0);
                    let y = Self::pop_number(args).unwrap_or(0.0);
                    let m = Self::pop_number(args).unwrap_or(0.0);
                    let c = Self::pop_number(args).unwrap_or(0.0);
                    self.do_K(c, m, y, k);
                }
            }
            Keyword::Kk => {
                if args.len() >= 4 {
                    let k = Self::pop_number(args).unwrap_or(0.0);
                    let y = Self::pop_number(args).unwrap_or(0.0);
                    let m = Self::pop_number(args).unwrap_or(0.0);
                    let c = Self::pop_number(args).unwrap_or(0.0);
                    self.do_k(c, m, y, k);
                }
            }
            Keyword::SC | Keyword::SCN => {
                // SC/SCN - set stroking color in current color space
                self.do_SC(args);
            }
            Keyword::Sc | Keyword::Scn => {
                // sc/scn - set non-stroking color in current color space
                self.do_sc(args);
            }

            // Clipping operators
            Keyword::WClip => self.do_W(),
            Keyword::WStar => self.do_W_star(),

            // Text object operators
            Keyword::BT => self.do_BT(),
            Keyword::ET => self.do_ET(),

            // Text state operators
            Keyword::Tc => {
                if let Some(cs) = Self::pop_number(args) {
                    self.do_Tc(cs);
                }
            }
            Keyword::Tw => {
                if let Some(ws) = Self::pop_number(args) {
                    self.do_Tw(ws);
                }
            }
            Keyword::Tz => {
                if let Some(s) = Self::pop_number(args) {
                    self.do_Tz(s);
                }
            }
            Keyword::TL => {
                if let Some(l) = Self::pop_number(args) {
                    self.do_TL(l);
                }
            }
            Keyword::Tf => {
                if args.len() >= 2 {
                    let size = Self::pop_number(args).unwrap_or(12.0);
                    let fontid = Self::pop_name(args).unwrap_or_default();
                    self.do_Tf(&fontid, size);
                }
            }
            Keyword::Tr => {
                if let Some(r) = Self::pop_int(args) {
                    self.do_Tr(r);
                }
            }
            Keyword::Ts => {
                if let Some(r) = Self::pop_number(args) {
                    self.do_Ts(r);
                }
            }

            // Text positioning operators
            Keyword::Td => {
                if let Some((tx, ty)) = Self::pop_point(args) {
                    self.do_Td(tx, ty);
                }
            }
            Keyword::TD => {
                if let Some((tx, ty)) = Self::pop_point(args) {
                    self.do_TD(tx, ty);
                }
            }
            Keyword::Tm => {
                if let Some((a, b, c, d, e, f)) = Self::pop_matrix(args) {
                    self.do_Tm(a, b, c, d, e, f);
                }
            }
            Keyword::TStar => self.do_T_star(),

            // Text showing operators
            Keyword::Tj => {
                if let Some(s) = Self::pop_string(args) {
                    self.do_Tj(s);
                }
            }
            Keyword::TJ => {
                if let Some(seq) = Self::pop_text_seq(args) {
                    self.do_TJ(seq);
                }
            }
            Keyword::Quote => {
                if let Some(s) = Self::pop_string(args) {
                    self.do_quote(s);
                }
            }
            Keyword::DoubleQuote => {
                if args.len() >= 3 {
                    let s = Self::pop_string(args).unwrap_or_default();
                    let ac = Self::pop_number(args).unwrap_or(0.0);
                    let aw = Self::pop_number(args).unwrap_or(0.0);
                    self.do_doublequote(aw, ac, s);
                }
            }

            // Unknown operator - ignore
            _ => {}
        }
    }

    // Helper functions to pop values from operand stack

    fn pop_number(args: &mut Vec<PSToken>) -> Option<f64> {
        args.pop().and_then(|t| match t {
            PSToken::Int(n) => Some(n as f64),
            PSToken::Real(n) => Some(n),
            _ => None,
        })
    }

    fn pop_int(args: &mut Vec<PSToken>) -> Option<i32> {
        args.pop().and_then(|t| match t {
            PSToken::Int(n) => Some(n as i32),
            PSToken::Real(n) => Some(n as i32),
            _ => None,
        })
    }

    fn pop_string(args: &mut Vec<PSToken>) -> Option<Vec<u8>> {
        args.pop().and_then(|t| match t {
            PSToken::String(s) => Some(s),
            _ => None,
        })
    }

    fn pop_name(args: &mut Vec<PSToken>) -> Option<String> {
        args.pop().and_then(|t| match t {
            PSToken::Literal(s) => Some(s),
            PSToken::Keyword(k) => std::str::from_utf8(k.as_bytes()).ok().map(String::from),
            _ => None,
        })
    }

    fn pop_array(args: &mut Vec<PSToken>) -> Option<Vec<f64>> {
        args.pop().and_then(|t| match t {
            PSToken::Array(arr) => Some(
                arr.iter()
                    .filter_map(|x| match x {
                        PSToken::Int(n) => Some(*n as f64),
                        PSToken::Real(n) => Some(*n),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => None,
        })
    }

    fn pop_point(args: &mut Vec<PSToken>) -> Option<(f64, f64)> {
        if args.len() >= 2 {
            let y = Self::pop_number(args)?;
            let x = Self::pop_number(args)?;
            Some((x, y))
        } else {
            None
        }
    }

    fn pop_matrix(args: &mut Vec<PSToken>) -> Option<(f64, f64, f64, f64, f64, f64)> {
        if args.len() >= 6 {
            let f = Self::pop_number(args)?;
            let e = Self::pop_number(args)?;
            let d = Self::pop_number(args)?;
            let c = Self::pop_number(args)?;
            let b = Self::pop_number(args)?;
            let a = Self::pop_number(args)?;
            Some((a, b, c, d, e, f))
        } else {
            None
        }
    }

    fn pop_text_seq(args: &mut Vec<PSToken>) -> Option<PDFTextSeq> {
        args.pop().and_then(|t| match t {
            PSToken::Array(arr) => {
                let seq: PDFTextSeq = arr
                    .into_iter()
                    .filter_map(|item| match item {
                        PSToken::Int(n) => Some(PDFTextSeqItem::Number(n as f64)),
                        PSToken::Real(n) => Some(PDFTextSeqItem::Number(n)),
                        PSToken::String(s) => Some(PDFTextSeqItem::Bytes(s)),
                        _ => None,
                    })
                    .collect();
                Some(seq)
            }
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_parse() {
        let stream = b"BT ET";
        let parser = PDFContentParser::new(vec![stream.to_vec()]);

        let tokens: Vec<_> = parser.collect();
        assert_eq!(tokens.len(), 2);
    }
}
