//! Arabic text reconstruction for PDF extraction.
//!
//! PDF text is often stored in visual page order. ICU performs the inverse
//! bidi operation. This module also keeps the output tied to source elements.

use std::cell::RefCell;

use unicode_bidi::BidiInfo;
use unicode_normalization::UnicodeNormalization;

use crate::layout::types::{Axis, TextLineElement};
use crate::utils::HasBBox;

const ICU_LTR: u8 = 0;
const ICU_RTL: u8 = 1;

/// The base direction selected for a reconstructed line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseDirection {
    Ltr,
    Rtl,
}

/// How much source evidence supports the reconstructed order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconstructionConfidence {
    Exact,
    Inferred,
    Ambiguous,
}

/// Logical text that came from one source layout element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconstructedSpan {
    pub source_index: usize,
    pub text: String,
}

/// One Rust-owned logical line result used by all output surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconstructedLine {
    pub raw_text: String,
    pub text: String,
    pub spans: Vec<ReconstructedSpan>,
    pub base_direction: BaseDirection,
    pub confidence: ReconstructionConfidence,
}

#[derive(Clone, Copy)]
struct SourceScalar {
    ch: char,
    source_index: usize,
}

struct IcuBidi {
    context: bolivar_icu::Bidi,
    input: Vec<u16>,
    output: Vec<u16>,
    output_to_input: Vec<i32>,
    input_to_source: Vec<usize>,
}

impl IcuBidi {
    fn new() -> Option<Self> {
        let context = bolivar_icu::Bidi::new()?;
        Some(Self {
            context,
            input: Vec::new(),
            output: Vec::new(),
            output_to_input: Vec::new(),
            input_to_source: Vec::new(),
        })
    }
    fn inverse(
        &mut self,
        source: &[SourceScalar],
        direction: BaseDirection,
    ) -> Option<Vec<ReconstructedSpan>> {
        self.input.clear();
        self.input_to_source.clear();
        for scalar in source {
            let mut encoded = [0; 2];
            let units = scalar.ch.encode_utf16(&mut encoded);
            self.input.extend_from_slice(units);
            self.input_to_source
                .extend(std::iter::repeat_n(scalar.source_index, units.len()));
        }

        self.output.resize(self.input.len(), 0);
        self.output_to_input.resize(self.input.len(), -1);
        let paragraph_level = match direction {
            BaseDirection::Ltr => ICU_LTR,
            BaseDirection::Rtl => ICU_RTL,
        };
        let output_length = self.context.inverse(
            &self.input,
            paragraph_level,
            &mut self.output,
            Some(&mut self.output_to_input),
        )?;
        if output_length > self.output.len() {
            return None;
        }

        mapped_utf16_to_spans(
            &self.output[..output_length],
            &self.output_to_input[..output_length],
            &self.input_to_source,
        )
    }

    fn inverse_text(&mut self, text: &str, direction: BaseDirection) -> Option<String> {
        self.input.clear();
        self.input.extend(text.encode_utf16());
        self.output.resize(self.input.len(), 0);
        let paragraph_level = match direction {
            BaseDirection::Ltr => ICU_LTR,
            BaseDirection::Rtl => ICU_RTL,
        };
        let output_length =
            self.context
                .inverse(&self.input, paragraph_level, &mut self.output, None)?;
        let output = String::from_utf16(self.output.get(..output_length)?).ok()?;
        Some(normalize_arabic_presentation_forms(&output))
    }
}

thread_local! {
    static ICU_BIDI: RefCell<Option<IcuBidi>> = RefCell::new(IcuBidi::new());
}

fn is_arabic(ch: char) -> bool {
    matches!(
        ch,
        '\u{0600}'..='\u{06FF}'
            | '\u{0750}'..='\u{077F}'
            | '\u{0870}'..='\u{089F}'
            | '\u{08A0}'..='\u{08FF}'
            | '\u{FB50}'..='\u{FDFF}'
            | '\u{FE70}'..='\u{FEFF}'
            | '\u{10E60}'..='\u{10E7F}'
            | '\u{1EE00}'..='\u{1EEFF}'
    )
}

fn is_rtl_script(ch: char) -> bool {
    is_arabic(ch) || matches!(ch, '\u{0590}'..='\u{05FF}' | '\u{FB1D}'..='\u{FB4F}')
}

fn is_arabic_presentation_form(ch: char) -> bool {
    matches!(ch, '\u{FB50}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}')
}

fn contains_rtl_script(text: &str) -> bool {
    text.chars().any(is_rtl_script)
}

fn select_base_direction(
    chars: impl IntoIterator<Item = char>,
) -> (BaseDirection, ReconstructionConfidence) {
    let mut has_rtl = false;
    let mut has_other_letters = false;
    let mut first_strong = None;
    let mut last_strong = None;
    let mut has_run_separator = false;

    for ch in chars {
        if is_rtl_script(ch) {
            has_rtl = true;
            first_strong.get_or_insert(BaseDirection::Rtl);
            last_strong = Some(BaseDirection::Rtl);
        } else if ch.is_alphabetic() {
            has_other_letters = true;
            first_strong.get_or_insert(BaseDirection::Ltr);
            last_strong = Some(BaseDirection::Ltr);
        } else if matches!(ch, '-' | '\u{2013}' | '\u{2014}' | '|') {
            has_run_separator = true;
        }
    }

    let separated_visual_rtl = has_run_separator
        && first_strong == Some(BaseDirection::Ltr)
        && last_strong == Some(BaseDirection::Rtl);
    let direction = if has_rtl && (!has_other_letters || separated_visual_rtl) {
        BaseDirection::Rtl
    } else {
        first_strong.unwrap_or(BaseDirection::Ltr)
    };
    let confidence = if has_rtl && has_other_letters {
        ReconstructionConfidence::Ambiguous
    } else if has_rtl {
        ReconstructionConfidence::Inferred
    } else {
        ReconstructionConfidence::Exact
    };
    (direction, confidence)
}

fn source_is_logical(elements: &[TextLineElement], axis: Axis) -> bool {
    if axis != Axis::Horizontal {
        return true;
    }

    let mut previous_x = None;
    let mut increasing = 0;
    let mut decreasing = 0;
    for element in elements {
        let TextLineElement::Char(character) = element else {
            continue;
        };
        if !character.get_text().chars().any(is_rtl_script) {
            continue;
        }

        let x = (character.x0() + character.x1()) * 0.5;
        if let Some(previous_x) = previous_x {
            let difference: f64 = x - previous_x;
            if difference > 0.01 {
                increasing += 1;
            } else if difference < -0.01 {
                decreasing += 1;
            }
        }
        previous_x = Some(x);
    }

    decreasing > increasing
}

fn source_scalars(elements: &[TextLineElement]) -> (String, Vec<SourceScalar>) {
    let mut raw_text = String::new();
    let mut source = Vec::new();
    for (source_index, element) in elements.iter().enumerate() {
        let text = match element {
            TextLineElement::Char(character) => character.get_text(),
            TextLineElement::Anno(annotation) => annotation.get_text(),
        };
        raw_text.push_str(text);
        source.extend(text.chars().map(|ch| SourceScalar { ch, source_index }));
    }
    (raw_text, source)
}

fn push_span(spans: &mut Vec<ReconstructedSpan>, source_index: usize, text: &str) {
    if let Some(last) = spans.last_mut()
        && last.source_index == source_index
    {
        last.text.push_str(text);
        return;
    }
    spans.push(ReconstructedSpan {
        source_index,
        text: text.to_owned(),
    });
}

fn push_normalized_scalar(spans: &mut Vec<ReconstructedSpan>, source_index: usize, ch: char) {
    if is_arabic_presentation_form(ch) {
        let mut source_buffer = [0; 4];
        for normalized in ch.encode_utf8(&mut source_buffer).nfkc() {
            let mut output_buffer = [0; 4];
            push_span(
                spans,
                source_index,
                normalized.encode_utf8(&mut output_buffer),
            );
        }
    } else {
        let mut encoded = [0; 4];
        push_span(spans, source_index, ch.encode_utf8(&mut encoded));
    }
}

fn normalized_source_spans(source: &[SourceScalar]) -> Vec<ReconstructedSpan> {
    let mut spans = Vec::with_capacity(source.len());
    for scalar in source {
        push_normalized_scalar(&mut spans, scalar.source_index, scalar.ch);
    }
    spans
}

fn mapped_utf16_to_spans(
    output: &[u16],
    output_to_input: &[i32],
    input_to_source: &[usize],
) -> Option<Vec<ReconstructedSpan>> {
    let mut spans = Vec::with_capacity(output.len());
    let mut offset = 0;
    while offset < output.len() {
        let first = output[offset];
        let width = if (0xD800..=0xDBFF).contains(&first) {
            2
        } else {
            1
        };
        let end = offset.checked_add(width)?;
        let units = output.get(offset..end)?;
        let ch = char::decode_utf16(units.iter().copied()).next()?.ok()?;

        let input_index = output_to_input[offset..end]
            .iter()
            .filter_map(|index| usize::try_from(*index).ok())
            .next()?;
        let source_index = *input_to_source.get(input_index)?;
        push_normalized_scalar(&mut spans, source_index, ch);
        offset = end;
    }
    Some(spans)
}

fn inverse_spans(
    source: &[SourceScalar],
    direction: BaseDirection,
) -> Option<Vec<ReconstructedSpan>> {
    ICU_BIDI.with(|cell| {
        let mut state = cell.try_borrow_mut().ok()?;
        state.as_mut()?.inverse(source, direction)
    })
}

fn inverse_text(text: &str, direction: BaseDirection) -> Option<String> {
    ICU_BIDI.with(|cell| {
        let mut state = cell.try_borrow_mut().ok()?;
        state.as_mut()?.inverse_text(text, direction)
    })
}

fn reconstruct_source(
    raw_text: String,
    source: Vec<SourceScalar>,
    already_logical: bool,
) -> ReconstructedLine {
    let (base_direction, confidence) = select_base_direction(source.iter().map(|scalar| scalar.ch));
    let spans = if !contains_rtl_script(&raw_text) || already_logical {
        normalized_source_spans(&source)
    } else {
        inverse_source_lines(&source, base_direction, confidence)
            .unwrap_or_else(|| normalized_source_spans(&source))
    };
    let text = spans.iter().map(|span| span.text.as_str()).collect();

    ReconstructedLine {
        raw_text,
        text,
        spans,
        base_direction,
        confidence,
    }
}

fn inverse_source_lines(
    source: &[SourceScalar],
    direction: BaseDirection,
    confidence: ReconstructionConfidence,
) -> Option<Vec<ReconstructedSpan>> {
    let mut output = Vec::new();
    let mut start = 0;
    for (index, scalar) in source.iter().enumerate() {
        if scalar.ch != '\n' {
            continue;
        }
        append_spans(
            &mut output,
            inverse_source_segment(&source[start..index], direction, confidence)?,
        );
        push_span(&mut output, scalar.source_index, "\n");
        start = index + 1;
    }
    append_spans(
        &mut output,
        inverse_source_segment(&source[start..], direction, confidence)?,
    );
    Some(output)
}

fn inverse_source_segment(
    source: &[SourceScalar],
    direction: BaseDirection,
    confidence: ReconstructionConfidence,
) -> Option<Vec<ReconstructedSpan>> {
    if direction != BaseDirection::Ltr || confidence != ReconstructionConfidence::Ambiguous {
        return inverse_spans(source, direction);
    }

    let mut output = Vec::new();
    let mut start = 0;
    while start < source.len() {
        let whitespace = source[start].ch.is_whitespace();
        let end = source[start..]
            .iter()
            .position(|scalar| scalar.ch.is_whitespace() != whitespace)
            .map_or(source.len(), |offset| start + offset);
        let segment = &source[start..end];
        if whitespace || !segment.iter().any(|scalar| is_rtl_script(scalar.ch)) {
            append_spans(&mut output, normalized_source_spans(segment));
        } else {
            let (segment_direction, _) =
                select_base_direction(segment.iter().map(|scalar| scalar.ch));
            append_spans(&mut output, inverse_spans(segment, segment_direction)?);
        }
        start = end;
    }
    Some(output)
}

fn append_spans(output: &mut Vec<ReconstructedSpan>, spans: Vec<ReconstructedSpan>) {
    for span in spans {
        push_span(output, span.source_index, &span.text);
    }
}

/// Reconstruct a layout line and retain its source-element mapping.
pub(crate) fn reconstruct_textline_elements(
    elements: &[TextLineElement],
    axis: Axis,
) -> ReconstructedLine {
    let (raw_text, source) = source_scalars(elements);
    reconstruct_source(raw_text, source, source_is_logical(elements, axis))
}
pub(crate) fn reconstruct_textline_text(elements: &[TextLineElement], axis: Axis) -> String {
    let raw_text = elements
        .iter()
        .map(|element| match element {
            TextLineElement::Char(character) => character.get_text(),
            TextLineElement::Anno(annotation) => annotation.get_text(),
        })
        .collect::<String>();
    if !contains_rtl_script(&raw_text) {
        return raw_text;
    }
    if source_is_logical(elements, axis) {
        return normalize_arabic_presentation_forms(&raw_text);
    }
    reconstruct_text_per_line(&raw_text)
}

fn reconstruct_string_line(line: &str) -> String {
    if !contains_rtl_script(line) {
        return line.to_owned();
    }
    let (direction, confidence) = select_base_direction(line.chars());
    if direction == BaseDirection::Ltr && confidence == ReconstructionConfidence::Ambiguous {
        return reconstruct_mixed_ltr_line(line);
    }
    inverse_text(line, direction).unwrap_or_else(|| normalize_arabic_presentation_forms(line))
}

fn reconstruct_mixed_ltr_line(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut start = 0;
    while start < line.len() {
        let whitespace = line[start..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace);
        let end = line[start..]
            .char_indices()
            .find(|(_, ch)| ch.is_whitespace() != whitespace)
            .map_or(line.len(), |(offset, _)| start + offset);
        let segment = &line[start..end];
        if whitespace || !contains_rtl_script(segment) {
            output.push_str(segment);
        } else {
            let (direction, _) = select_base_direction(segment.chars());
            output.push_str(
                &inverse_text(segment, direction)
                    .unwrap_or_else(|| normalize_arabic_presentation_forms(segment)),
            );
        }
        start = end;
    }
    output
}

/// Reconstruct visual PDF text one line at a time.
fn reconstruct_text_per_line(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    if !contains_rtl_script(text) {
        return text.to_owned();
    }

    let mut output = String::with_capacity(text.len());
    for chunk in text.split_inclusive('\n') {
        if let Some(line) = chunk.strip_suffix('\n') {
            output.push_str(&reconstruct_string_line(line));
            output.push('\n');
        } else {
            output.push_str(&reconstruct_string_line(chunk));
        }
    }
    output
}

/// Normalize only Arabic presentation-form code points.
pub(crate) fn normalize_arabic_presentation_forms(text: &str) -> String {
    if !text.chars().any(is_arabic_presentation_form) {
        return text.to_owned();
    }

    let mut output = String::with_capacity(text.len());
    for ch in text.chars() {
        if is_arabic_presentation_form(ch) {
            let mut buffer = [0; 4];
            output.extend(ch.encode_utf8(&mut buffer).nfkc());
        } else {
            output.push(ch);
        }
    }
    output
}

/// Return final logical and nominal Arabic text.
pub fn reconstruct_text_for_output(text: &str) -> String {
    reconstruct_text_per_line(text)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WordBidiClass {
    Rtl,
    Ltr,
    Neutral,
}

fn word_bidi_class(text: &str) -> WordBidiClass {
    if contains_rtl_script(text) {
        return WordBidiClass::Rtl;
    }
    if text.chars().any(char::is_alphabetic) {
        return WordBidiClass::Ltr;
    }
    WordBidiClass::Neutral
}

/// Reorder left-to-right geometric word runs with the legacy policy.
pub(crate) fn reorder_visual_word_runs<T>(
    words: Vec<T>,
    text: impl for<'a> Fn(&'a T) -> &'a str,
) -> Vec<T> {
    if !words.iter().any(|word| contains_rtl_script(text(word))) {
        return words;
    }

    let mut runs: Vec<(WordBidiClass, Vec<T>)> = Vec::new();
    for word in words {
        let class = word_bidi_class(text(&word));
        if let Some((last_class, run)) = runs.last_mut()
            && *last_class == class
        {
            run.push(word);
            continue;
        }
        runs.push((class, vec![word]));
    }

    for (class, run) in &mut runs {
        if *class == WordBidiClass::Rtl {
            run.reverse();
        }
    }
    runs.reverse();

    runs.into_iter().flat_map(|(_, run)| run).collect()
}

/// Reorder text with the legacy UAX #9 policy.
pub fn reorder_text_per_line(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    let mut output = String::with_capacity(text.len());
    for chunk in text.split_inclusive('\n') {
        let (line, has_newline) = match chunk.strip_suffix('\n') {
            Some(prefix) => (prefix, true),
            None => (chunk, false),
        };

        if !line.is_empty() {
            output.push_str(&reorder_single_line(line));
        }
        if has_newline {
            output.push('\n');
        }
    }
    output
}

fn reorder_single_line(line: &str) -> String {
    let info = BidiInfo::new(line, None);
    if info.paragraphs.is_empty() {
        return line.to_owned();
    }

    let mut output = String::with_capacity(line.len());
    for paragraph in &info.paragraphs {
        output.push_str(&info.reorder_line(paragraph, paragraph.range.clone()));
    }
    output
}

fn contains_arabic_presentation_forms(text: &str) -> bool {
    text.chars().any(is_arabic_presentation_form)
}

/// Normalize Arabic forms with the legacy compatibility behavior.
pub fn normalize_presentation_forms_for_output(text: &str) -> String {
    if text.is_empty() || !contains_arabic_presentation_forms(text) {
        return text.to_owned();
    }
    text.nfkc().collect()
}

/// Reorder and normalize text with the legacy output policy.
pub fn reorder_text_for_output(text: &str) -> String {
    normalize_presentation_forms_for_output(&reorder_text_per_line(text))
}

/// Reorder geometric text with the legacy output policy.
pub fn reorder_visual_text_for_output(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    let mut output = String::with_capacity(text.len());
    for chunk in text.split_inclusive('\n') {
        let (line, has_newline) = match chunk.strip_suffix('\n') {
            Some(prefix) => (prefix, true),
            None => (chunk, false),
        };

        if contains_rtl_script(line) {
            let words = line
                .split_whitespace()
                .map(reorder_text_for_output)
                .collect::<Vec<_>>();
            let reordered = reorder_visual_word_runs(words, String::as_str);
            output.push_str(&reordered.join(" "));
        } else {
            output.push_str(line);
        }
        if has_newline {
            output.push('\n');
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{LTAnno, LTChar};

    fn horizontal_elements(text: &str, decreasing: bool) -> Vec<TextLineElement> {
        let count = text.chars().count() as f64;
        let mut elements = text
            .chars()
            .enumerate()
            .map(|(index, ch)| {
                let x = if decreasing {
                    count - index as f64
                } else {
                    index as f64
                };
                TextLineElement::Char(Box::new(LTChar::new(
                    (x, 0.0, x + 0.9, 1.0),
                    &ch.to_string(),
                    "F",
                    10.0,
                    true,
                    1.0,
                )))
            })
            .collect::<Vec<_>>();
        elements.push(TextLineElement::Anno(LTAnno::new("\n")));
        elements
    }

    #[test]
    fn empty_text_stays_empty() {
        assert_eq!(reconstruct_text_per_line(""), "");
    }

    #[test]
    fn ltr_text_is_unchanged() {
        assert_eq!(reconstruct_text_per_line("abc 123"), "abc 123");
    }

    #[test]
    fn preserves_newline_boundaries() {
        assert_eq!(reconstruct_text_per_line("abc\n123\n"), "abc\n123\n");
    }

    #[test]
    fn arabic_visual_line_reorders_to_logical_and_keeps_digits() {
        let line = "1120280977 :ﻊﺟﺮﻤﻟﺍ ﻢﻗﺭ";
        assert_eq!(reconstruct_text_per_line(line), "رقم المرجع: 1120280977");
    }

    #[test]
    fn arabic_visual_words_reorder_to_logical() {
        assert_eq!(reconstruct_text_per_line("ﺏﺎﺴﺤﻟﺍ ﻒﺸﻛ"), "كشف الحساب");
    }

    #[test]
    fn mixed_ltr_line_keeps_latin_prefix() {
        assert_eq!(
            reconstruct_text_per_line("abc 123 ﺔﻴﺑﺮﻌﻟﺍ"),
            "abc 123 العربية"
        );
    }

    #[test]
    fn mixed_rtl_line_uses_arabic_visual_start() {
        assert_eq!(
            reconstruct_text_per_line("ﺔﻴﺑﺮﻌﻟﺍ 123 abc"),
            "abc 123 العربية"
        );
    }

    #[test]
    fn normalization_does_not_change_other_compatibility_characters() {
        assert_eq!(normalize_arabic_presentation_forms("① ﺏ"), "① ب");
    }

    #[test]
    fn element_mapping_handles_repeated_visual_glyphs() {
        let elements = horizontal_elements("ﺏﺏ", false);
        let line = reconstruct_textline_elements(&elements, Axis::Horizontal);

        assert_eq!(line.text, "بب\n");
        assert_eq!(line.spans[0].source_index, 1);
        assert_eq!(line.spans[1].source_index, 0);
    }

    #[test]
    fn element_mapping_keeps_lam_alef_expansion_on_one_source() {
        let elements = horizontal_elements("ﻻ", false);
        let line = reconstruct_textline_elements(&elements, Axis::Horizontal);

        assert_eq!(line.text, "لا\n");
        assert_eq!(line.spans[0].source_index, 0);
        assert_eq!(line.spans[0].text, "لا");
    }

    #[test]
    fn element_mapping_points_to_each_output_source() {
        let elements = horizontal_elements("1120280977 :ﻊﺟﺮﻤﻟﺍ ﻢﻗﺭ", false);
        let line = reconstruct_textline_elements(&elements, Axis::Horizontal);

        assert_eq!(line.text, "رقم المرجع: 1120280977\n");
        for span in line.spans {
            let source_text = match &elements[span.source_index] {
                TextLineElement::Char(character) => character.get_text(),
                TextLineElement::Anno(annotation) => annotation.get_text(),
            };
            assert_eq!(span.text, normalize_arabic_presentation_forms(source_text));
        }
    }

    #[test]
    fn decreasing_arabic_geometry_preserves_logical_source_order() {
        let elements = horizontal_elements("كشف", true);
        let line = reconstruct_textline_elements(&elements, Axis::Horizontal);

        assert_eq!(line.text, "كشف\n");
        assert_eq!(line.confidence, ReconstructionConfidence::Inferred);
    }

    #[test]
    fn mixed_source_is_marked_ambiguous() {
        let elements = horizontal_elements("abc ﺔﻴﺑﺮﻌﻟﺍ", false);
        let line = reconstruct_textline_elements(&elements, Axis::Horizontal);

        assert_eq!(line.text, "abc العربية\n");
        assert_eq!(line.base_direction, BaseDirection::Ltr);
        assert_eq!(line.confidence, ReconstructionConfidence::Ambiguous);
    }

    #[test]
    fn reconstructs_al_rajhi_compact_mixed_fields() {
        let visual = "P1 Term 6467299202155588 07/02 14:47:05:ﺔﻈﺣﻼﻣ**23:11:18:ﺖﻗﻮﻟﺍ";
        assert_eq!(
            reconstruct_text_for_output(visual),
            "P1 Term 6467299202155588 07/02 الوقت:23:11:18**ملاحظة:14:47:05"
        );
    }

    #[test]
    fn legacy_functions_keep_previous_output() {
        assert_eq!(
            reorder_text_per_line("1120280977 :ﻊﺟﺮﻤﻟﺍ ﻢﻗﺭ"),
            "ﺭﻗﻢ ﺍﻟﻤﺮﺟﻊ: 1120280977"
        );
        assert_eq!(reorder_text_for_output("clinics# ليوحت"), "clinics# تحويل");
        assert_eq!(normalize_presentation_forms_for_output("① ﺏ"), "1 ب");
    }
}
