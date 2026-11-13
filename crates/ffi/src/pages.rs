//! Build one batch per page. Each builder follows the traversal, text joins
//! and bidi rules of the structs the UniFFI binding returned, but writes
//! columns directly.

use std::collections::BTreeSet;

use bolivar_core::engine::PageTables;
use bolivar_core::layout::{
    LTChar, LTItem, LTPage, LTTextBox, LTTextLine, TextBoxType, TextLineElement,
    reorder_text_for_output,
};
use bolivar_core::pdfpage::PDFPage;
use bolivar_core::table::{BBox as TableBBox, PageGeometry, TableMetadata};
use bolivar_core::utils::HasBBox;

use crate::abi::FfiResult;
use crate::batch::{
    Batch, Builder, NONE, layout, page_text, raw, summary, table_rows, tables, to_i32,
};
use crate::geometry::table_bbox_in_pdf_space;
use crate::text::{has_decimal_digit, java_blank, java_round, page_number};

// String table slots, so each kind of value keeps its own last-hit cache.
const FONT: usize = 0;
const ORIENTATION: usize = 1;
const MODE: usize = 2;
const TAG: usize = 3;
const NCS: usize = 4;
const SCS: usize = 5;

const NEWLINE: u16 = b'\n' as u16;

fn bbox4(rect: (f64, f64, f64, f64)) -> [f64; 4] {
    [rect.0, rect.1, rect.2, rect.3]
}

/// Every text box of a page, depth first through figures, as the UniFFI
/// builders collected them.
fn text_boxes(page: &LTPage) -> Vec<&TextBoxType> {
    fn walk<'a>(item: &'a LTItem, out: &mut Vec<&'a TextBoxType>) {
        match item {
            LTItem::TextBox(text_box) => out.push(text_box),
            LTItem::Figure(figure) => figure.iter().for_each(|child| walk(child, out)),
            LTItem::Page(page) => page.iter().for_each(|child| walk(child, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    page.iter().for_each(|item| walk(item, &mut out));
    out
}

/// Sizes for presizing the columns.
#[derive(Default)]
struct Counts {
    lines: usize,
    chars: usize,
    text: usize,
}

fn count(boxes: &[&TextBoxType]) -> Counts {
    let mut counts = Counts::default();
    let mut add = |elements: &[TextLineElement]| {
        counts.lines += 1;
        for element in elements {
            if let TextLineElement::Char(character) = element {
                counts.chars += 1;
                counts.text += character.get_text().len();
            }
        }
    };
    for text_box in boxes {
        match text_box {
            TextBoxType::Horizontal(b) => b.iter().for_each(|line| add(line.elements())),
            TextBoxType::Vertical(b) => b.iter().for_each(|line| add(line.elements())),
        }
    }
    counts
}

/// Apply `$body` to each line of a text box, whatever its orientation.
macro_rules! each_line {
    ($text_box:expr, |$line:ident, $vertical:ident| $body:expr) => {
        match $text_box {
            TextBoxType::Horizontal(b) => {
                let $vertical = false;
                for $line in b.iter() {
                    $body;
                }
            }
            TextBoxType::Vertical(b) => {
                let $vertical = true;
                for $line in b.iter() {
                    $body;
                }
            }
        }
    };
}

fn text_box_bbox(text_box: &TextBoxType) -> [f64; 4] {
    match text_box {
        TextBoxType::Horizontal(b) => bbox4(b.bbox()),
        TextBoxType::Vertical(b) => bbox4(b.bbox()),
    }
}

fn writing_mode(text_box: &TextBoxType) -> &'static str {
    match text_box {
        TextBoxType::Horizontal(b) => b.get_writing_mode(),
        TextBoxType::Vertical(b) => b.get_writing_mode(),
    }
}

fn orientation(vertical: bool) -> &'static str {
    if vertical { "vertical" } else { "horizontal" }
}

// ---- shared by layout and raw ----------------------------------------------

/// The columns layout and raw batches share for boxes, lines and characters.
#[derive(Clone, Copy)]
struct Shared {
    box_bbox: usize,
    box_mode: usize,
    box_text: usize,
    box_lines: usize,
    line_bbox: usize,
    line_orient: usize,
    line_text: usize,
    line_chars: usize,
    char_x0: usize,
    char_y0: usize,
    char_x1: usize,
    char_y1: usize,
    char_size: usize,
    char_font: usize,
    char_upright: usize,
    char_text: usize,
}

macro_rules! shared_columns {
    ($kind:ident) => {
        Shared {
            box_bbox: $kind::Col::BoxBbox as usize,
            box_mode: $kind::Col::BoxMode as usize,
            box_text: $kind::Col::BoxText as usize,
            box_lines: $kind::Col::BoxLines as usize,
            line_bbox: $kind::Col::LineBbox as usize,
            line_orient: $kind::Col::LineOrient as usize,
            line_text: $kind::Col::LineText as usize,
            line_chars: $kind::Col::LineChars as usize,
            char_x0: $kind::Col::CharX0 as usize,
            char_y0: $kind::Col::CharY0 as usize,
            char_x1: $kind::Col::CharX1 as usize,
            char_y1: $kind::Col::CharY1 as usize,
            char_size: $kind::Col::CharSize as usize,
            char_font: $kind::Col::CharFont as usize,
            char_upright: $kind::Col::CharUpright as usize,
            char_text: $kind::Col::CharText as usize,
        }
    };
}

const LAYOUT: Shared = shared_columns!(layout);
const RAW: Shared = shared_columns!(raw);

/// A text line of either orientation.
trait Line: LTTextLine + LineSource {}
impl<T: LTTextLine + LineSource> Line for T {}

/// Writes one line's own columns; the shared ones are written around it.
trait LineWriter {
    fn write<L: Line>(&mut self, b: &mut Builder, line: &L) -> FfiResult<()>;
}

impl Shared {
    fn reserve(self, b: &mut Builder, boxes: usize, counts: &Counts) {
        b.reserve(&[self.box_bbox], 4 * boxes);
        b.reserve(&[self.box_mode, self.box_lines], boxes + 1);
        b.reserve(&[self.box_text], 2 * boxes);
        b.reserve(&[self.line_bbox], 4 * counts.lines);
        b.reserve(&[self.line_orient, self.line_chars], counts.lines + 1);
        b.reserve(&[self.line_text], 2 * counts.lines);
        b.reserve(
            &[
                self.char_x0,
                self.char_y0,
                self.char_x1,
                self.char_y1,
                self.char_size,
                self.char_font,
                self.char_upright,
            ],
            counts.chars,
        );
        b.reserve(&[self.char_text], 2 * counts.chars);
    }

    /// Write every box and its lines, joining box texts with newlines into
    /// the page's contiguous text. Returns the end of that text.
    fn boxes(
        self,
        b: &mut Builder,
        boxes: &[&TextBoxType],
        mode: impl Fn(&TextBoxType) -> &'static str,
        writer: &mut impl LineWriter,
    ) -> FfiResult<i32> {
        b.i32s(self.box_lines).push(0);
        b.i32s(self.line_chars).push(0);
        for (index, text_box) in boxes.iter().enumerate() {
            if index > 0 {
                b.text_unit(NEWLINE);
            }
            let start = b.text_pos()?;
            each_line!(text_box, |line, vertical| self
                .line(b, line, vertical, writer)?);
            let end = b.text_pos()?;
            b.f64s(self.box_bbox).extend(text_box_bbox(text_box));
            let mode = b.intern(MODE, mode(text_box))?;
            b.i32s(self.box_mode).push(mode);
            b.i32s(self.box_text).extend([start, end]);
            let lines = to_i32(b.i32s(self.line_orient).len(), "lines")?;
            b.i32s(self.box_lines).push(lines);
        }
        b.text_pos()
    }

    fn line<L: Line>(
        self,
        b: &mut Builder,
        line: &L,
        vertical: bool,
        writer: &mut impl LineWriter,
    ) -> FfiResult<()> {
        b.f64s(self.line_bbox).extend(bbox4(line.bbox()));
        let orient = b.intern(ORIENTATION, orientation(vertical))?;
        b.i32s(self.line_orient).push(orient);
        writer.write(b, line)?;
        let chars = to_i32(b.i32s(self.char_font).len(), "chars")?;
        b.i32s(self.line_chars).push(chars);
        Ok(())
    }

    /// Write one character; its text goes to aux.
    fn char(self, b: &mut Builder, text: &str, character: &LTChar) -> FfiResult<()> {
        let (x0, y0, x1, y1) = character.bbox();
        b.f64s(self.char_x0).push(x0);
        b.f64s(self.char_y0).push(y0);
        b.f64s(self.char_x1).push(x1);
        b.f64s(self.char_y1).push(y1);
        b.f64s(self.char_size).push(character.size());
        let font = b.intern(FONT, character.fontname())?;
        b.i32s(self.char_font).push(font);
        b.u8s(self.char_upright).push(u8::from(character.upright()));
        let range = b.aux(text)?;
        b.i32s(self.char_text).extend(range);
        Ok(())
    }
}

// ---- layout ----------------------------------------------------------------

/// The layout page: boxes, lines and characters. Under bidi, characters are
/// the reconstructed spans in logical order, as `LayoutChar` was.
pub fn layout_batch(page: &LTPage) -> FfiResult<Batch> {
    use layout::Col as C;
    let mut b = Builder::new(layout::KIND, layout::COLUMNS);
    let boxes = text_boxes(page);
    let counts = count(&boxes);
    LAYOUT.reserve(&mut b, boxes.len(), &counts);
    b.reserve_text(counts.text + boxes.len() + counts.lines, counts.text);
    let mode = |text_box: &TextBoxType| match text_box {
        TextBoxType::Vertical(_) => "tb-rl",
        TextBoxType::Horizontal(_) => "lr-tb",
    };
    let end = LAYOUT.boxes(&mut b, &boxes, mode, &mut LayoutLine)?;
    b.i64s(C::Ints)
        .extend([page_number(page.pageid), 0, i64::from(end)]);
    b.f64s(C::Floats).push(page.rotate);
    b.f64s(C::Floats).extend(bbox4(page.bbox()));
    b.finish(&[C::CharText.into()])
}

/// A layout line's text and characters, bidi-reconstructed when bidi is on.
struct LayoutLine;

impl LineWriter for LayoutLine {
    fn write<L: Line>(&mut self, b: &mut Builder, line: &L) -> FfiResult<()> {
        let elements = line.source_elements();
        if line.bidi_enabled() {
            let reconstructed = line.reconstructed_line();
            let range = b.text(&reconstructed.text)?;
            b.i32s(LAYOUT.line_text).extend(range);
            for span in &reconstructed.spans {
                if let Some(TextLineElement::Char(character)) = elements.get(span.source_index) {
                    LAYOUT.char(b, &span.text, character)?;
                }
            }
        } else {
            let range = b.text(&line.get_text())?;
            b.i32s(LAYOUT.line_text).extend(range);
            for element in elements {
                if let TextLineElement::Char(character) = element {
                    LAYOUT.char(b, character.get_text(), character)?;
                }
            }
        }
        Ok(())
    }
}

/// What the builders need from a text line, for both orientations.
pub trait LineSource {
    fn source_elements(&self) -> &[TextLineElement];
    fn bidi_enabled(&self) -> bool;
    fn reconstructed_line(&self) -> bolivar_core::layout::ReconstructedLine;
}

macro_rules! line_source {
    ($ty:ty) => {
        impl LineSource for $ty {
            fn source_elements(&self) -> &[TextLineElement] {
                self.elements()
            }
            fn bidi_enabled(&self) -> bool {
                self.bidi()
            }
            fn reconstructed_line(&self) -> bolivar_core::layout::ReconstructedLine {
                self.reconstructed()
            }
        }
    };
}
line_source!(bolivar_core::layout::LTTextLineHorizontal);
line_source!(bolivar_core::layout::LTTextLineVertical);

// ---- raw -------------------------------------------------------------------

/// The lossless page: PDF identity, boxes, lines with raw and processed
/// text, every source glyph with its graphics state, and the page's tables
/// in pdfplumber space.
pub fn raw_batch(
    page_index: usize,
    pdf_page: &PDFPage,
    page: &LTPage,
    tables: &[TableMetadata],
) -> FfiResult<Batch> {
    use raw::Col as C;
    let mut b = Builder::new(raw::KIND, raw::COLUMNS);
    let boxes = text_boxes(page);
    let counts = count(&boxes);
    RAW.reserve(&mut b, boxes.len(), &counts);
    b.reserve(
        &[
            C::CharAdvance.into(),
            C::CharMcid.into(),
            C::CharTag.into(),
            C::CharNcs.into(),
            C::CharScs.into(),
        ],
        counts.chars,
    );
    b.reserve(
        &[C::CharNcolor.into(), C::CharScolor.into()],
        2 * counts.chars,
    );
    b.reserve(&[C::CharMatrix.into()], 6 * counts.chars);
    b.reserve_text(
        counts.text + boxes.len() + counts.lines,
        2 * counts.text + counts.lines,
    );
    b.i32s(C::TableCells).push(0);
    let text_end = RAW.boxes(&mut b, &boxes, writing_mode, &mut RawLine)?;
    let label = b.text_opt(pdf_page.label.as_deref())?;

    for table in tables {
        b.f64s(C::TableBbox).extend([
            table.bbox.x0,
            table.bbox.top,
            table.bbox.x1,
            table.bbox.bottom,
        ]);
        b.i32s(C::TableShape).extend(shape(table)?);
        for cell in &table.cells {
            b.i32s(C::CellIndex).extend(cell_index(cell)?);
            b.f64s(C::CellBbox).extend([
                cell.bbox.x0,
                cell.bbox.top,
                cell.bbox.x1,
                cell.bbox.bottom,
            ]);
            let text = b.text(&cell.text)?;
            b.i32s(C::CellText).extend(text);
        }
        let cells = to_i32(b.i32s(C::CellIndex).len() / 4, "cells")?;
        b.i32s(C::TableCells).push(cells);
    }

    for page_box in [
        pdf_page.mediabox,
        pdf_page.cropbox,
        pdf_page.bleedbox,
        pdf_page.trimbox,
        pdf_page.artbox,
    ] {
        b.u8s(C::PageBoxPresent).push(u8::from(page_box.is_some()));
        if let Some(values) = page_box {
            b.f64s(C::PageBoxes).extend(values);
        }
    }
    b.i64s(C::Ints).extend([
        page_index as i64,
        page_index as i64 + 1,
        i64::from(pdf_page.pageid),
        pdf_page.rotate,
        i64::from(label[0]),
        i64::from(label[1]),
        0,
        i64::from(text_end),
    ]);
    b.f64s(C::Floats).push(pdf_page.user_unit);
    b.f64s(C::Floats).extend(bbox4(page.bbox()));
    b.finish(&[C::CharText.into(), C::LineRawText.into()])
}

fn shape(table: &TableMetadata) -> FfiResult<[i32; 2]> {
    Ok([
        to_i32(table.row_count, "rows")?,
        to_i32(table.column_count, "columns")?,
    ])
}

fn cell_index(cell: &bolivar_core::table::TableCellMetadata) -> FfiResult<[i32; 4]> {
    Ok([
        to_i32(cell.row_index, "row index")?,
        to_i32(cell.column_index, "column index")?,
        to_i32(cell.row_span, "row span")?,
        to_i32(cell.column_span, "column span")?,
    ])
}

/// A raw line's raw text, output text and source glyphs.
struct RawLine;

impl LineWriter for RawLine {
    fn write<L: Line>(&mut self, b: &mut Builder, line: &L) -> FfiResult<()> {
        use raw::Col as C;
        // Raw text is every element's text, annotations included.
        let mut raw_text = String::new();
        for element in line.source_elements() {
            match element {
                TextLineElement::Char(character) => {
                    raw_text.push_str(character.get_text());
                    raw_char(b, character)?;
                }
                TextLineElement::Anno(annotation) => raw_text.push_str(annotation.get_text()),
            }
        }
        let raw_range = b.aux(&raw_text)?;
        b.i32s(C::LineRawText).extend(raw_range);
        let text = if line.bidi_enabled() {
            line.get_text()
        } else {
            reorder_text_for_output(&raw_text)
        };
        let range = b.text(&text)?;
        b.i32s(C::LineText).extend(range);
        Ok(())
    }
}

fn raw_char(b: &mut Builder, character: &LTChar) -> FfiResult<()> {
    use raw::Col as C;
    RAW.char(b, character.get_text(), character)?;
    b.f64s(C::CharAdvance).push(character.adv());
    let m = character.matrix();
    b.f64s(C::CharMatrix).extend([m.0, m.1, m.2, m.3, m.4, m.5]);
    b.i32s(C::CharMcid)
        .push(character.mcid().unwrap_or(i32::MIN));
    let tag = b.intern_opt(TAG, character.tag_str())?;
    b.i32s(C::CharTag).push(tag);
    let ncs = b.intern_opt(NCS, character.ncs_str())?;
    b.i32s(C::CharNcs).push(ncs);
    let scs = b.intern_opt(SCS, character.scs_str())?;
    b.i32s(C::CharScs).push(scs);
    for (color, col) in [
        (character.non_stroking_color(), C::CharNcolor),
        (character.stroking_color(), C::CharScolor),
    ] {
        let range = match color {
            Some(values) => {
                let start = b.f64s(C::ColorPool).len();
                b.f64s(C::ColorPool).extend_from_slice(values);
                [
                    to_i32(start, "colors")?,
                    to_i32(start + values.len(), "colors")?,
                ]
            }
            None => NONE,
        };
        b.i32s(col).extend(range);
    }
    Ok(())
}

// ---- page text -------------------------------------------------------------

#[derive(Default)]
struct GlyphSummary {
    fonts: BTreeSet<String>,
    digit_styles: BTreeSet<(String, i32)>,
}

impl GlyphSummary {
    fn add(&mut self, character: &LTChar) {
        let text = character.get_text();
        if java_blank(text) {
            return;
        }
        let font_name = character.fontname();
        if !self.fonts.contains(font_name) {
            self.fonts.insert(font_name.to_owned());
        }
        if has_decimal_digit(text) {
            self.digit_styles
                .insert((font_name.to_owned(), java_round(character.size())));
        }
    }
}

/// A page's boxes and lines with and without bidi, and a summary of its
/// visible glyphs: the fonts, and the font and whole size of each digit.
pub fn page_text_batch(page_index: usize, page: &LTPage) -> FfiResult<Batch> {
    use page_text::Col as C;
    let mut b = Builder::new(page_text::KIND, page_text::COLUMNS);
    let mut glyphs = GlyphSummary::default();
    b.i32s(C::BoxLines).push(0);
    for text_box in text_boxes(page) {
        let start = b.text_pos()?;
        let aux_start = b.aux_pos()?;
        each_line!(text_box, |line, _vertical| page_text_line(
            &mut b,
            line,
            &mut glyphs
        )?);
        let end = b.text_pos()?;
        let aux_end = b.aux_pos()?;
        b.f64s(C::BoxBbox).extend(text_box_bbox(text_box));
        b.i32s(C::BoxText).extend([start, end]);
        b.i32s(C::BoxTextWithoutBidi).extend([aux_start, aux_end]);
        let lines = to_i32(b.i32s(C::LineText).len() / 2, "lines")?;
        b.i32s(C::BoxLines).push(lines);
    }
    for font in &glyphs.fonts {
        let id = b.intern(FONT, font)?;
        b.i32s(C::Fonts).push(id);
    }
    for (font, size) in &glyphs.digit_styles {
        let id = b.intern(FONT, font)?;
        b.i32s(C::DigitFont).push(id);
        b.i32s(C::DigitSize).push(*size);
    }
    b.i64s(C::Ints).push(page_index as i64 + 1);
    b.f64s(C::Floats).extend(bbox4(page.bbox()));
    b.finish(&[C::BoxTextWithoutBidi.into(), C::LineTextWithoutBidi.into()])
}

fn page_text_line<L: LTTextLine + HasBBox + LineSource>(
    b: &mut Builder,
    line: &L,
    glyphs: &mut GlyphSummary,
) -> FfiResult<()> {
    use page_text::Col as C;
    let mut raw_text = String::new();
    for element in line.source_elements() {
        match element {
            TextLineElement::Char(character) => {
                raw_text.push_str(character.get_text());
                glyphs.add(character);
            }
            TextLineElement::Anno(annotation) => raw_text.push_str(annotation.get_text()),
        }
    }
    let without_bidi = reorder_text_for_output(&raw_text);
    let text = if line.bidi_enabled() {
        b.text(&line.get_text())?
    } else {
        b.text(&without_bidi)?
    };
    let aux = b.aux(&without_bidi)?;
    b.f64s(C::LineBbox).extend(bbox4(line.bbox()));
    b.i32s(C::LineText).extend(text);
    b.i32s(C::LineTextWithoutBidi).extend(aux);
    Ok(())
}

// ---- summary, tables, table rows ------------------------------------------

/// The page number, box, rotation and text of one page.
pub fn summary_batch(page: &LTPage) -> FfiResult<Batch> {
    use summary::Col as C;
    let mut b = Builder::new(summary::KIND, summary::COLUMNS);
    let text = b.text(&page.get_text())?;
    b.i64s(C::Ints).extend([
        page_number(page.pageid),
        i64::from(text[0]),
        i64::from(text[1]),
    ]);
    b.f64s(C::Floats).push(page.rotate);
    b.f64s(C::Floats).extend(bbox4(page.bbox()));
    b.finish(&[])
}

fn pdf_bbox(bbox: TableBBox, geometry: &PageGeometry) -> [f64; 4] {
    bbox4(table_bbox_in_pdf_space(bbox, geometry))
}

/// The tables of one page, with boxes in PDF space.
pub fn tables_batch(
    page_index: usize,
    geometry: &PageGeometry,
    page_tables: Vec<TableMetadata>,
) -> FfiResult<Batch> {
    use tables::Col as C;
    let mut b = Builder::new(tables::KIND, tables::COLUMNS);
    b.i32s(C::TableCells).push(0);
    for table in page_tables {
        b.f64s(C::TableBbox).extend(pdf_bbox(table.bbox, geometry));
        b.i32s(C::TableShape).extend(shape(&table)?);
        for cell in table.cells {
            b.i32s(C::CellIndex).extend(cell_index(&cell)?);
            b.f64s(C::CellBbox).extend(pdf_bbox(cell.bbox, geometry));
            let text = b.text(&cell.text)?;
            b.i32s(C::CellText).extend(text);
        }
        let cells = to_i32(b.i32s(C::CellIndex).len() / 4, "cells")?;
        b.i32s(C::TableCells).push(cells);
    }
    b.i64s(C::Ints).push(page_index as i64 + 1);
    b.finish(&[])
}

/// The raw table rows of one page (absent cells are absent ranges). Row
/// offsets index cells; table offsets index rows; both end with the total.
pub fn table_rows_batch(page_index: usize, page_tables: PageTables) -> FfiResult<Batch> {
    use table_rows::Col as C;
    let mut b = Builder::new(table_rows::KIND, table_rows::COLUMNS);
    b.i32s(C::RowOffsets).push(0);
    b.i32s(C::TableOffsets).push(0);
    let mut cells = 0usize;
    for table in page_tables {
        for row in table {
            for cell in row {
                let range = b.text_opt(cell.as_deref())?;
                b.i32s(C::CellText).extend(range);
                cells += 1;
            }
            b.i32s(C::RowOffsets).push(to_i32(cells, "cells")?);
        }
        let rows = b.i32s(C::RowOffsets).len() - 1;
        b.i32s(C::TableOffsets).push(to_i32(rows, "rows")?);
    }
    b.i64s(C::Ints).push(page_index as i64 + 1);
    b.finish(&[])
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::batch::{Column, common};
    use bolivar_core::layout::LTTextLineHorizontal;
    use bolivar_core::pdftypes::PDFDict;
    use bolivar_core::table::TableCellMetadata;

    /// Read a finished batch in tests.
    pub struct View<'a>(pub &'a Batch, pub &'a [(&'static str, crate::batch::Elem)]);

    impl View<'_> {
        fn index(&self, name: &str) -> usize {
            self.1
                .iter()
                .position(|(n, _)| *n == name)
                .unwrap_or_else(|| panic!("{name}"))
        }
        pub fn f64s(&self, name: &str) -> &[f64] {
            match &self.0.columns()[self.index(name)] {
                Column::F64(v) => v,
                _ => panic!("{name}"),
            }
        }
        pub fn i32s(&self, name: &str) -> &[i32] {
            match &self.0.columns()[self.index(name)] {
                Column::I32(v) => v,
                _ => panic!("{name}"),
            }
        }
        pub fn i64s(&self, name: &str) -> &[i64] {
            match &self.0.columns()[self.index(name)] {
                Column::I64(v) => v,
                _ => panic!("{name}"),
            }
        }
        pub fn u8s(&self, name: &str) -> &[u8] {
            match &self.0.columns()[self.index(name)] {
                Column::U8(v) => v,
                _ => panic!("{name}"),
            }
        }
        pub fn text(&self, start: i32, end: i32) -> String {
            let Column::U16(text) = &self.0.columns()[common::TEXT] else {
                panic!()
            };
            String::from_utf16(&text[start as usize..end as usize]).unwrap()
        }
        pub fn ranges(&self, name: &str) -> Vec<Option<String>> {
            self.i32s(name)
                .chunks(2)
                .map(|r| (r[0] >= 0).then(|| self.text(r[0], r[1])))
                .collect()
        }
        pub fn string(&self, id: i32) -> String {
            let Column::U16(strings) = &self.0.columns()[common::STRINGS] else {
                panic!()
            };
            let Column::I32(off) = &self.0.columns()[common::STRING_OFF] else {
                panic!()
            };
            let id = id as usize;
            String::from_utf16(&strings[off[id] as usize..off[id + 1] as usize]).unwrap()
        }
    }

    fn line(glyphs: &[(&str, &str, f64)]) -> LTTextLineHorizontal {
        let mut line = LTTextLineHorizontal::new(0.1);
        for (index, (glyph, font, size)) in glyphs.iter().enumerate() {
            let x = index as f64;
            line.add_element(TextLineElement::Char(Box::new(LTChar::new(
                (x, 0.0, x + 1.0, 1.0),
                glyph,
                font,
                *size,
                true,
                1.0,
            ))));
        }
        line
    }

    fn arabic_line(bidi: bool) -> LTTextLineHorizontal {
        let visual = ["ﺔ", "ﻴ", "ﺑ", "ﺮ", "ﻌ", "ﻟ", "ﺍ"];
        let mut line = line(&visual.map(|g| (g, "F", 10.0)));
        line.analyze();
        line.set_bidi(bidi);
        line
    }

    #[test]
    fn layout_line_text_uses_icu_mapping_when_enabled() {
        let line = arabic_line(true);
        let mut b = Builder::new(layout::KIND, layout::COLUMNS);
        b.i32s(layout::Col::LineChars).push(0);
        LAYOUT.line(&mut b, &line, false, &mut LayoutLine).unwrap();
        let batch = b.finish(&[layout::Col::CharText.into()]).unwrap();
        let v = View(&batch, layout::COLUMNS);
        assert_eq!(v.ranges("LineText"), vec![Some("العربية\n".to_owned())]);
        let chars: String = v
            .ranges("CharText")
            .into_iter()
            .map(Option::unwrap)
            .collect();
        assert_eq!(chars, "العربية");

        let mut b = Builder::new(raw::KIND, raw::COLUMNS);
        b.i32s(raw::Col::LineChars).push(0);
        RAW.line(&mut b, &line, false, &mut RawLine).unwrap();
        let batch = b
            .finish(&[raw::Col::CharText.into(), raw::Col::LineRawText.into()])
            .unwrap();
        let v = View(&batch, raw::COLUMNS);
        assert_eq!(v.ranges("LineRawText"), vec![Some("ﺔﻴﺑﺮﻌﻟﺍ\n".to_owned())]);
        assert_eq!(v.ranges("LineText"), vec![Some("العربية\n".to_owned())]);
        assert_eq!(v.ranges("CharText")[0].as_deref(), Some("ﺔ"));
    }

    #[test]
    fn page_text_summarises_visible_characters() {
        let line = line(&[
            ("A", "F1", 10.0),
            ("\u{00A0}", "F2", 10.0),
            (" ", "F3", 10.0),
            ("٣", "F4", 8.5),
            ("7", "F1", 11.4),
        ]);
        let mut glyphs = GlyphSummary::default();
        let mut b = Builder::new(page_text::KIND, page_text::COLUMNS);
        page_text_line(&mut b, &line, &mut glyphs).unwrap();
        let batch = b
            .finish(&[page_text::Col::LineTextWithoutBidi.into()])
            .unwrap();
        let v = View(&batch, page_text::COLUMNS);
        assert_eq!(v.ranges("LineText"), v.ranges("LineTextWithoutBidi"));
        assert_eq!(
            glyphs.fonts.into_iter().collect::<Vec<_>>(),
            ["F1", "F2", "F4"]
        );
        assert_eq!(
            glyphs.digit_styles.into_iter().collect::<Vec<_>>(),
            [("F1".to_owned(), 11), ("F4".to_owned(), 9)]
        );
    }

    #[test]
    fn page_text_line_keeps_both_texts_under_bidi() {
        let run = |bidi| {
            let line = arabic_line(bidi);
            let mut b = Builder::new(page_text::KIND, page_text::COLUMNS);
            page_text_line(&mut b, &line, &mut GlyphSummary::default()).unwrap();
            let batch = b
                .finish(&[page_text::Col::LineTextWithoutBidi.into()])
                .unwrap();
            let v = View(&batch, page_text::COLUMNS);
            (
                v.ranges("LineText")[0].clone().unwrap(),
                v.ranges("LineTextWithoutBidi")[0].clone().unwrap(),
            )
        };
        let (without_text, without_plain) = run(false);
        let (with_text, with_plain) = run(true);
        assert_eq!(with_text, "العربية\n");
        assert_eq!(with_plain, without_text);
        assert_eq!(without_text, without_plain);
    }

    #[test]
    fn raw_character_preserves_matrix_marked_content_and_colors() {
        let character = LTChar::builder((1.0, 2.0, 3.0, 4.0), "A", "Helvetica", 12.0)
            .upright(true)
            .adv(7.5)
            .matrix((1.0, 0.1, 0.2, 1.0, 20.0, 30.0))
            .mcid(Some(17))
            .tag(Some("Span".to_owned()))
            .ncs(Some("DeviceRGB".to_owned()))
            .scs(Some("DeviceGray".to_owned()))
            .non_stroking_color(Some(vec![0.1, 0.2, 0.3]))
            .stroking_color(Some(vec![0.4]))
            .build();
        let mut b = Builder::new(raw::KIND, raw::COLUMNS);
        raw_char(&mut b, &character).unwrap();
        let batch = b.finish(&[raw::Col::CharText.into()]).unwrap();
        let v = View(&batch, raw::COLUMNS);
        assert_eq!(v.ranges("CharText")[0].as_deref(), Some("A"));
        assert_eq!(v.f64s("CharAdvance"), [7.5]);
        assert_eq!(v.f64s("CharMatrix"), [1.0, 0.1, 0.2, 1.0, 20.0, 30.0]);
        assert_eq!(v.i32s("CharMcid"), [17]);
        assert_eq!(v.string(v.i32s("CharTag")[0]), "Span");
        assert_eq!(v.string(v.i32s("CharNcs")[0]), "DeviceRGB");
        assert_eq!(v.string(v.i32s("CharScs")[0]), "DeviceGray");
        let pool = v.f64s("ColorPool");
        let n = v.i32s("CharNcolor");
        let s = v.i32s("CharScolor");
        assert_eq!(&pool[n[0] as usize..n[1] as usize], [0.1, 0.2, 0.3]);
        assert_eq!(&pool[s[0] as usize..s[1] as usize], [0.4]);
    }

    #[test]
    fn raw_page_preserves_pdf_identity_boxes_and_table_coordinates() {
        let page = PDFPage {
            pageid: 42,
            attrs: PDFDict::default(),
            label: Some("iv".to_owned()),
            mediabox: Some([0.0, 0.0, 200.0, 300.0]),
            cropbox: Some([10.0, 20.0, 190.0, 280.0]),
            bleedbox: None,
            trimbox: Some([7.0, 8.0, 193.0, 292.0]),
            artbox: None,
            rotate: 90,
            annots: None,
            resources: PDFDict::default(),
            contents: Vec::new(),
            user_unit: 2.0,
        };
        let layout_page = LTPage::new(1, (0.0, 0.0, 300.0, 200.0), 90.0);
        let bbox = TableBBox {
            x0: 11.0,
            top: 22.0,
            x1: 99.0,
            bottom: 88.0,
        };
        let table = TableMetadata {
            bbox,
            row_count: 1,
            column_count: 1,
            cells: vec![TableCellMetadata {
                row_index: 0,
                column_index: 0,
                row_span: 1,
                column_span: 1,
                bbox,
                text: "value".to_owned(),
                text_spans: Vec::new(),
            }],
        };
        let batch = raw_batch(3, &page, &layout_page, &[table]).unwrap();
        let v = View(&batch, raw::COLUMNS);
        let ints = v.i64s("Ints");
        assert_eq!(&ints[..4], [3, 4, 42, 90]);
        assert_eq!(v.text(ints[4] as i32, ints[5] as i32), "iv");
        assert_eq!(v.f64s("Floats")[0], 2.0);
        assert_eq!(v.u8s("PageBoxPresent"), [1, 1, 0, 1, 0]);
        assert_eq!(
            &v.f64s("PageBoxes")[..8],
            [0.0, 0.0, 200.0, 300.0, 10.0, 20.0, 190.0, 280.0]
        );
        assert_eq!(v.f64s("TableBbox"), [11.0, 22.0, 99.0, 88.0]);
        assert_eq!(v.f64s("CellBbox")[1], 22.0);
        assert_eq!(v.ranges("CellText"), vec![Some("value".to_owned())]);
        assert_eq!(v.i32s("TableCells"), [0, 1]);
    }

    #[test]
    fn table_rows_flatten_tables_with_empty_cells() {
        let batch = table_rows_batch(
            2,
            vec![
                vec![vec![Some("a".to_owned()), None], Vec::new()],
                vec![vec![None, Some("b".to_owned())]],
            ],
        )
        .unwrap();
        let v = View(&batch, table_rows::COLUMNS);
        assert_eq!(v.i64s("Ints"), [3]);
        assert_eq!(
            v.ranges("CellText"),
            vec![Some("a".to_owned()), None, None, Some("b".to_owned())]
        );
        assert_eq!(v.i32s("RowOffsets"), [0, 2, 2, 4]);
        assert_eq!(v.i32s("TableOffsets"), [0, 2, 3]);
    }

    #[test]
    fn table_rows_preserve_empty_tables_and_rows() {
        let batch = table_rows_batch(0, vec![Vec::new(), vec![Vec::new()]]).unwrap();
        let v = View(&batch, table_rows::COLUMNS);
        assert!(v.i32s("CellText").is_empty());
        assert_eq!(v.i32s("RowOffsets"), [0, 0]);
        assert_eq!(v.i32s("TableOffsets"), [0, 0, 1]);
    }
}
