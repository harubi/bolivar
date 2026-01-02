//! PDF parser - converts PostScript tokens to PDF objects.
//!
//! Port of pdfminer.six pdfparser.py

use crate::error::{PdfError, Result};
use crate::pdftypes::{PDFObjRef, PDFObject};
use crate::psparser::{Keyword, PSBaseParser, PSToken};
use std::collections::HashMap;

/// PDF Parser - parses PDF object syntax
///
/// Uses PSBaseParser for tokenization and builds PDF objects,
/// handling indirect references (num num R) appropriately.
pub struct PDFParser<'a> {
    base: PSBaseParser<'a>,
    /// Lookahead buffer for tokens
    lookahead: Vec<PSToken>,
}

impl<'a> PDFParser<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            base: PSBaseParser::new(data),
            lookahead: Vec::new(),
        }
    }

    /// Get remaining unparsed data.
    pub fn remaining(&self) -> &[u8] {
        self.base.remaining()
    }

    /// Get next token (from lookahead or parser)
    fn next_token(&mut self) -> Result<Option<PSToken>> {
        if let Some(tok) = self.lookahead.pop() {
            return Ok(Some(tok));
        }
        match self.base.next_token() {
            Some(Ok((_, tok))) => Ok(Some(tok)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    /// Push token back to lookahead
    fn push_back(&mut self, tok: PSToken) {
        self.lookahead.push(tok);
    }

    /// Parse next PDF object
    pub fn parse_object(&mut self) -> Result<PDFObject> {
        let token = self.next_token()?.ok_or(PdfError::UnexpectedEof)?;
        self.token_to_object(token)
    }

    /// Convert a token to a PDF object
    fn token_to_object(&mut self, token: PSToken) -> Result<PDFObject> {
        match token {
            PSToken::Int(n) => {
                // Could be start of indirect reference: objid genno R
                if let Ok(Some(tok2)) = self.next_token() {
                    if let PSToken::Int(m) = tok2 {
                        if let Ok(Some(tok3)) = self.next_token() {
                            if let PSToken::Keyword(Keyword::R) = tok3 {
                                return Ok(PDFObject::Ref(PDFObjRef::new(n as u32, m as u32)));
                            }
                            // Not R, push back both
                            self.push_back(tok3);
                        }
                        self.push_back(PSToken::Int(m));
                    } else {
                        self.push_back(tok2);
                    }
                }
                Ok(PDFObject::Int(n))
            }
            PSToken::Real(n) => Ok(PDFObject::Real(n)),
            PSToken::Bool(b) => Ok(PDFObject::Bool(b)),
            PSToken::Literal(s) => Ok(PDFObject::Name(s)),
            PSToken::String(s) => Ok(PDFObject::String(s)),
            PSToken::Keyword(kw) => match kw {
                Keyword::Null => Ok(PDFObject::Null),
                Keyword::True => Ok(PDFObject::Bool(true)),
                Keyword::False => Ok(PDFObject::Bool(false)),
                Keyword::ArrayStart => self.parse_array(),
                Keyword::DictStart => self.parse_dict(),
                Keyword::Unknown(bytes) => {
                    let name = String::from_utf8_lossy(&bytes).to_string();
                    Ok(PDFObject::Name(name))
                }
                _ => Err(PdfError::TokenError {
                    pos: self.base.tell(),
                    msg: format!("unexpected keyword: {:?}", kw),
                }),
            },
            PSToken::Array(_) | PSToken::Dict(_) => {
                // These shouldn't come from base parser
                Err(PdfError::TokenError {
                    pos: self.base.tell(),
                    msg: "unexpected compound token".into(),
                })
            }
        }
    }

    /// Parse array contents until ]
    fn parse_array(&mut self) -> Result<PDFObject> {
        let mut arr = Vec::new();

        loop {
            let token = self.next_token()?.ok_or(PdfError::UnexpectedEof)?;

            if let PSToken::Keyword(Keyword::ArrayEnd) = token {
                break;
            }

            arr.push(self.token_to_object(token)?);
        }

        Ok(PDFObject::Array(arr))
    }

    /// Parse dict contents until >>
    fn parse_dict(&mut self) -> Result<PDFObject> {
        let mut dict = HashMap::new();

        loop {
            let token = self.next_token()?.ok_or(PdfError::UnexpectedEof)?;

            // Check for end of dict
            if let PSToken::Keyword(Keyword::DictEnd) = token {
                break;
            }

            // Key must be a literal name
            let key = match token {
                PSToken::Literal(name) => name,
                _ => {
                    return Err(PdfError::TokenError {
                        pos: self.base.tell(),
                        msg: "expected name as dict key".into(),
                    });
                }
            };

            // Parse value
            let value = self.parse_object()?;
            dict.insert(key, value);
        }

        Ok(PDFObject::Dict(dict))
    }
}

/// Content stream operation
#[derive(Debug, Clone)]
pub struct Operation {
    /// The operator (e.g., BT, Tf, Tj)
    pub operator: Keyword,
    /// Operands for this operation
    pub operands: Vec<PDFObject>,
}

/// PDF Content Stream Parser
///
/// Parses PDF content streams into a sequence of operations.
/// Each operation consists of an operator and its operands.
pub struct PDFContentParser;

impl PDFContentParser {
    /// Parse a content stream into operations
    pub fn parse(data: &[u8]) -> Result<Vec<Operation>> {
        let mut parser = PSBaseParser::new(data);
        let mut ops = Vec::new();
        let mut operands: Vec<PDFObject> = Vec::new();
        let mut context_stack: Vec<Vec<PDFObject>> = Vec::new();

        while let Some(result) = parser.next_token() {
            let (_, token) = result?;

            match token {
                PSToken::Keyword(kw) => {
                    // Special handling for array/dict delimiters
                    match kw {
                        Keyword::ArrayStart => {
                            context_stack.push(std::mem::take(&mut operands));
                            continue;
                        }
                        Keyword::ArrayEnd => {
                            let array_contents = std::mem::take(&mut operands);
                            operands = context_stack.pop().unwrap_or_default();
                            operands.push(PDFObject::Array(array_contents));
                            continue;
                        }
                        Keyword::DictStart => {
                            context_stack.push(std::mem::take(&mut operands));
                            continue;
                        }
                        Keyword::DictEnd => {
                            let dict_contents = std::mem::take(&mut operands);
                            operands = context_stack.pop().unwrap_or_default();
                            // Convert to dict
                            let mut dict = HashMap::new();
                            let mut iter = dict_contents.into_iter();
                            while let Some(key) = iter.next() {
                                if let PDFObject::Name(name) = key {
                                    if let Some(value) = iter.next() {
                                        dict.insert(name, value);
                                    }
                                }
                            }
                            operands.push(PDFObject::Dict(dict));
                            continue;
                        }
                        Keyword::BI => {
                            // Handle inline image (BI ... ID ... EI)
                            // Collect until ID
                            let mut img_params = Vec::new();
                            while let Some(Ok((_, tok))) = parser.next_token() {
                                if let PSToken::Keyword(Keyword::ID) = &tok {
                                    break;
                                }
                                if let Ok(obj) = Self::ps_to_pdf(tok) {
                                    img_params.push(obj);
                                }
                            }
                            // Convert params to dict
                            let mut dict = HashMap::new();
                            let mut iter = img_params.into_iter();
                            while let Some(key) = iter.next() {
                                if let PDFObject::Name(name) = key {
                                    if let Some(value) = iter.next() {
                                        dict.insert(name, value);
                                    }
                                }
                            }
                            ops.push(Operation {
                                operator: Keyword::BI,
                                operands: vec![PDFObject::Dict(dict)],
                            });
                            // Skip until EI (simplified - in real impl would need to read raw bytes)
                            while let Some(Ok((_, tok))) = parser.next_token() {
                                if let PSToken::Keyword(Keyword::EI) = &tok {
                                    ops.push(Operation {
                                        operator: Keyword::EI,
                                        operands: vec![],
                                    });
                                    break;
                                }
                            }
                            continue;
                        }
                        _ => {
                            // Regular operator - emit operation
                            ops.push(Operation {
                                operator: kw,
                                operands: std::mem::take(&mut operands),
                            });
                        }
                    }
                }
                // Accumulate operands
                _ => {
                    if let Ok(obj) = Self::ps_to_pdf(token) {
                        operands.push(obj);
                    }
                }
            }
        }

        Ok(ops)
    }

    /// Convert PSToken to PDFObject
    fn ps_to_pdf(token: PSToken) -> Result<PDFObject> {
        match token {
            PSToken::Int(n) => Ok(PDFObject::Int(n)),
            PSToken::Real(n) => Ok(PDFObject::Real(n)),
            PSToken::Bool(b) => Ok(PDFObject::Bool(b)),
            PSToken::Literal(s) => Ok(PDFObject::Name(s)),
            PSToken::String(s) => Ok(PDFObject::String(s)),
            PSToken::Array(arr) => {
                let objs: Result<Vec<PDFObject>> = arr.into_iter().map(Self::ps_to_pdf).collect();
                Ok(PDFObject::Array(objs?))
            }
            PSToken::Dict(d) => {
                let mut map = HashMap::new();
                for (k, v) in d {
                    map.insert(k, Self::ps_to_pdf(v)?);
                }
                Ok(PDFObject::Dict(map))
            }
            PSToken::Keyword(kw) => match kw {
                Keyword::Null => Ok(PDFObject::Null),
                Keyword::True => Ok(PDFObject::Bool(true)),
                Keyword::False => Ok(PDFObject::Bool(false)),
                _ => Err(PdfError::TokenError {
                    pos: 0,
                    msg: format!("unexpected keyword: {:?}", kw),
                }),
            },
        }
    }
}
