//! pdfminer.six's stack based object reader, for recovering damaged files.
//!
//! Bolivar's [`PDFParser`](super::PDFParser) is a strict recursive descent
//! parser. pdfminer.six instead pushes tokens onto a stack and only yields
//! objects when `endobj` (or `xref` / `startxref`) closes them; dictionaries
//! pair whatever tokens they hold in order. That leniency lets pdfminer.six
//! read objects behind a damaged header, for example the xref stream of
//! `issue-1249-evil-xrefs.pdf`. This reader ports `PSStackParser.nextobject`
//! with `PDFParser.do_keyword`, and is used only after strict parsing failed.
//!
//! pdfminer.six keeps bare keywords inside arrays and dictionaries.
//! `PDFObject` has no keyword variant, so they become strings there: like a
//! keyword, a string reads as 0 through `int_value` and is never a name.

use std::collections::VecDeque;

use super::lexer::{Keyword, PSBaseParser, PSToken};
use crate::error::{PdfError, Result};
use crate::pdftypes::{PDFObjRef, PDFObject, PDFStream};
use rustc_hash::FxHashMap;

/// One stack entry: an object, or a keyword that pdfminer.six keeps as is.
#[derive(Debug)]
enum Entry {
    Object(PDFObject),
    Keyword(Keyword),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Frame {
    Array,
    Dict,
    Proc,
}

pub struct StackObjectReader<'a> {
    data: &'a [u8],
    lexer: PSBaseParser<'a>,
    frame: Option<Frame>,
    stack: Vec<Entry>,
    context: Vec<(Option<Frame>, Vec<Entry>)>,
    results: VecDeque<Entry>,
    /// pdfminer.six's `parser.fallback`: once a document needed the fallback
    /// scan, streams ignore `/Length` and run to `endstream`.
    fallback: bool,
}

impl<'a> StackObjectReader<'a> {
    pub fn new(data: &'a [u8], pos: usize) -> Self {
        let mut lexer = PSBaseParser::new(data);
        lexer.set_pos(pos);
        Self {
            data,
            lexer,
            frame: None,
            stack: Vec::new(),
            context: Vec::new(),
            results: VecDeque::new(),
            fallback: false,
        }
    }

    #[must_use]
    pub const fn with_fallback(mut self, fallback: bool) -> Self {
        self.fallback = fallback;
        self
    }

    /// The next raw token, like pdfminer.six's `nexttoken`.
    pub fn next_token(&mut self) -> Result<(usize, PSToken)> {
        self.lexer.next_token().ok_or(PdfError::UnexpectedEof)?
    }

    /// The next complete object, like pdfminer.six's `nextobject`.
    ///
    /// `resolve_int` resolves an indirect `/Length`; pdfminer.six reads it
    /// with `int_value`, so anything that is not an integer counts as 0.
    pub fn next_object(
        &mut self,
        resolve_int: impl Fn(&PDFObjRef) -> Option<i64>,
    ) -> Result<PDFObject> {
        while self.results.is_empty() {
            let (pos, token) = self.next_token()?;
            match token {
                PSToken::Keyword(Keyword::ArrayStart) => self.start(Frame::Array),
                PSToken::Keyword(Keyword::ArrayEnd) => {
                    if let Some(objs) = self.end(Frame::Array) {
                        self.push_object(PDFObject::Array(objects(objs)));
                    }
                }
                PSToken::Keyword(Keyword::DictStart) => self.start(Frame::Dict),
                PSToken::Keyword(Keyword::DictEnd) => {
                    if let Some(objs) = self.end(Frame::Dict) {
                        let dict = pair_dict(objs)?;
                        self.push_object(PDFObject::Dict(dict));
                    }
                }
                PSToken::Keyword(Keyword::BraceOpen) => self.start(Frame::Proc),
                PSToken::Keyword(Keyword::BraceClose) => {
                    if let Some(objs) = self.end(Frame::Proc) {
                        self.push_object(PDFObject::Array(objects(objs)));
                    }
                }
                PSToken::Keyword(keyword) => self.do_keyword(pos, keyword, &resolve_int)?,
                PSToken::Int(n) => self.push_object(PDFObject::Int(n)),
                PSToken::Real(n) => self.push_object(PDFObject::Real(n)),
                PSToken::Bool(b) => self.push_object(PDFObject::Bool(b)),
                PSToken::Literal(name) => self.push_object(PDFObject::Name(name)),
                PSToken::String(s) => self.push_object(PDFObject::String(s)),
                PSToken::Array(_) | PSToken::Dict(_) => {}
            }
        }
        match self.results.pop_front() {
            Some(Entry::Object(obj)) => Ok(obj),
            _ => Err(PdfError::SyntaxError(
                "recovered a keyword, not an object".into(),
            )),
        }
    }

    fn push_object(&mut self, obj: PDFObject) {
        self.stack.push(Entry::Object(obj));
    }

    fn start(&mut self, frame: Frame) {
        let outer = std::mem::take(&mut self.stack);
        self.context.push((self.frame, outer));
        self.frame = Some(frame);
    }

    /// Closes `frame`; a mismatched closer is ignored, as in non-strict mode.
    fn end(&mut self, frame: Frame) -> Option<Vec<Entry>> {
        if self.frame != Some(frame) {
            return None;
        }
        let (outer_frame, outer) = self.context.pop()?;
        self.frame = outer_frame;
        Some(std::mem::replace(&mut self.stack, outer))
    }

    /// pdfminer.six's `PSStackParser.reset`.
    fn reset(&mut self) {
        self.frame = None;
        self.stack.clear();
        self.context.clear();
        self.results.clear();
    }

    fn pop(&mut self, n: usize) -> Vec<Entry> {
        let at = self.stack.len().saturating_sub(n);
        self.stack.split_off(at)
    }

    fn do_keyword(
        &mut self,
        pos: usize,
        keyword: Keyword,
        resolve_int: &impl Fn(&PDFObjRef) -> Option<i64>,
    ) -> Result<()> {
        match keyword {
            Keyword::Xref | Keyword::StartXref => {
                let popped = self.pop(1);
                self.results.extend(popped);
            }
            Keyword::EndObj => {
                let popped = self.pop(4);
                self.results.extend(popped);
            }
            Keyword::Null => self.push_object(PDFObject::Null),
            Keyword::True => self.push_object(PDFObject::Bool(true)),
            Keyword::False => self.push_object(PDFObject::Bool(false)),
            Keyword::R => {
                if self.stack.len() >= 2 {
                    let popped = self.pop(2);
                    // pdfminer.six's `safe_int`; an id that does not fit here
                    // still keeps its place, as object 0, which never exists.
                    let objid = match popped.first() {
                        Some(Entry::Object(PDFObject::Int(n))) => Some(*n),
                        Some(Entry::Object(PDFObject::Real(n))) => Some(*n as i64),
                        _ => None,
                    };
                    if let Some(objid) = objid {
                        let objid = u32::try_from(objid).unwrap_or(0);
                        self.push_object(PDFObject::Ref(PDFObjRef::new(objid, 0)));
                    }
                }
            }
            Keyword::Stream => self.read_stream(pos, resolve_int)?,
            other => self.stack.push(Entry::Keyword(other)),
        }
        Ok(())
    }

    /// pdfminer.six's `stream` handling: `/Length` bytes after the `stream`
    /// line, then lines up to `endstream` move the cursor without adding data.
    fn read_stream(
        &mut self,
        pos: usize,
        resolve_int: &impl Fn(&PDFObjRef) -> Option<i64>,
    ) -> Result<()> {
        let dict = match self.pop(1).pop() {
            Some(Entry::Object(PDFObject::Dict(dict))) => dict,
            Some(Entry::Object(_)) => FxHashMap::default(),
            _ => return Err(PdfError::SyntaxError("Invalid stream dictionary".into())),
        };
        let length = match dict.get("Length") {
            _ if self.fallback => 0,
            Some(PDFObject::Int(n)) => *n,
            Some(PDFObject::Ref(r)) => resolve_int(r).unwrap_or(0),
            _ => 0,
        };
        // pdfminer.six seeks to the `stream` keyword here, and seeking resets
        // its stack parser: everything before the stream is discarded.
        self.reset();
        let Some(line_end) = next_line_end(self.data, pos) else {
            return Ok(());
        };
        let start = line_end;
        let length = usize::try_from(length).unwrap_or(0);
        let end = start.saturating_add(length).min(self.data.len());
        let mut rawdata = self.data[start..end].to_vec();

        let mut objlen = end - start;
        let mut cursor = end;
        while let Some(next) = next_line_end(self.data, cursor) {
            let line = &self.data[cursor..next];
            if let Some(i) = find(line, b"endstream") {
                objlen += i;
                if self.fallback {
                    rawdata.extend_from_slice(&line[..i]);
                }
                break;
            }
            objlen += line.len();
            if self.fallback {
                rawdata.extend_from_slice(line);
            }
            cursor = next;
        }
        self.lexer.set_pos(start + objlen);
        self.push_object(PDFObject::Stream(Box::new(PDFStream::new(dict, rawdata))));
        Ok(())
    }
}

fn objects(entries: Vec<Entry>) -> Vec<PDFObject> {
    entries.into_iter().map(Entry::into_object).collect()
}

impl Entry {
    fn into_object(self) -> PDFObject {
        match self {
            Self::Object(obj) => obj,
            Self::Keyword(keyword) => PDFObject::String(keyword.as_bytes().to_vec()),
        }
    }
}

/// pdfminer.six pairs dictionary tokens in order and drops `null` values;
/// an odd count is an error even in non-strict mode.
fn pair_dict(entries: Vec<Entry>) -> Result<FxHashMap<smol_str::SmolStr, PDFObject>> {
    if entries.len() % 2 != 0 {
        return Err(PdfError::SyntaxError("Invalid dictionary construct".into()));
    }
    let mut dict = FxHashMap::default();
    let mut iter = entries.into_iter();
    while let (Some(key), Some(value)) = (iter.next(), iter.next()) {
        let key = match key {
            Entry::Object(PDFObject::Name(name)) => name,
            Entry::Object(PDFObject::Int(n)) => n.to_string().into(),
            Entry::Object(PDFObject::Real(n)) => n.to_string().into(),
            Entry::Object(PDFObject::String(s)) => String::from_utf8_lossy(&s).into(),
            _ => continue,
        };
        match value.into_object() {
            PDFObject::Null => {}
            value => {
                dict.insert(key, value);
            }
        }
    }
    Ok(dict)
}

/// End of the line that starts at `pos`, past its `\r`, `\n` or `\r\n`,
/// like pdfminer.six's `nextline`.
fn next_line_end(data: &[u8], pos: usize) -> Option<usize> {
    if pos >= data.len() {
        return None;
    }
    let rest = &data[pos..];
    match rest.iter().position(|&b| b == b'\r' || b == b'\n') {
        Some(i) if rest[i] == b'\r' && rest.get(i + 1) == Some(&b'\n') => Some(pos + i + 2),
        Some(i) => Some(pos + i + 1),
        None => Some(data.len()),
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_refs(_: &PDFObjRef) -> Option<i64> {
        None
    }

    #[test]
    fn reads_stream_object_after_any_three_tokens() {
        let data = b"9 junk <> 1 0 obj\n<< /Type /XRef /Size 2 /W [1 1 1] /Root 3 0 R /Length 3 >>\nstream\nabc\nendstream\nendobj\n";
        let mut reader = StackObjectReader::new(data, 0);
        for _ in 0..3 {
            reader.next_token().unwrap();
        }
        let obj = reader.next_object(no_refs).unwrap();
        let stream = obj.as_stream().unwrap();
        assert_eq!(stream.get("Type"), Some(&PDFObject::Name("XRef".into())));
        assert_eq!(
            stream.get("Root"),
            Some(&PDFObject::Ref(PDFObjRef::new(3, 0)))
        );
        assert_eq!(stream.get_rawdata(), b"abc");
    }

    // Values from pdfminer.six reading the same bytes the same way.
    #[test]
    fn recovers_the_evil_xrefs_stream_like_pdfminer() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../references/pdfminer.six/samples/contrib/issue-1249-evil-xrefs.pdf"
        );
        let data = std::fs::read(path).unwrap();
        let mut reader = StackObjectReader::new(&data, 0);
        for _ in 0..3 {
            reader.next_token().unwrap();
        }
        let obj = reader.next_object(no_refs).unwrap();
        let stream = obj.as_stream().unwrap();
        let int = |n| PDFObject::Int(n);
        assert_eq!(stream.get("Type"), Some(&PDFObject::Name("XRef".into())));
        assert_eq!(stream.get("Size"), Some(&int(16)));
        assert_eq!(
            stream.get("Index"),
            Some(&PDFObject::Array(vec![int(11), int(5)]))
        );
        assert_eq!(
            stream.get("W"),
            Some(&PDFObject::Array(vec![int(1), int(2), int(0)]))
        );
        assert_eq!(
            stream.get("Root"),
            Some(&PDFObject::Ref(PDFObjRef::new(12, 0)))
        );
        assert_eq!(stream.get_rawdata().len(), 30);
    }

    #[test]
    fn pairs_dictionary_entries_in_order_like_pdfminer() {
        // A non-name key keeps the pairing, and null values are dropped.
        let data = b"<< 3 /E /A null /B 2 >> endobj";
        let mut reader = StackObjectReader::new(data, 0);
        let PDFObject::Dict(dict) = reader.next_object(no_refs).unwrap() else {
            panic!("expected a dictionary");
        };
        assert_eq!(dict.get("3"), Some(&PDFObject::Name("E".into())));
        assert_eq!(dict.get("A"), None);
        assert_eq!(dict.get("B"), Some(&PDFObject::Int(2)));
    }

    #[test]
    fn oversized_reference_keeps_its_place() {
        let data = b"<< /Parent 106987615545692 0 R /Type /Page >> endobj";
        let mut reader = StackObjectReader::new(data, 0);
        let PDFObject::Dict(dict) = reader.next_object(no_refs).unwrap() else {
            panic!("expected a dictionary");
        };
        assert_eq!(
            dict.get("Parent"),
            Some(&PDFObject::Ref(PDFObjRef::new(0, 0)))
        );
        assert_eq!(dict.get("Type"), Some(&PDFObject::Name("Page".into())));
    }

    #[test]
    fn odd_dictionary_is_an_error() {
        let mut reader = StackObjectReader::new(b"<< /A >> endobj", 0);
        assert!(reader.next_object(no_refs).is_err());
    }

    #[test]
    fn eof_before_a_result_is_an_error() {
        let mut reader = StackObjectReader::new(b"1 0 obj << /A 1 >>", 0);
        assert!(reader.next_object(no_refs).is_err());
    }
}
