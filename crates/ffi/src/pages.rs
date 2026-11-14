//! Build one batch per page. Builders run on the stream's workers, so pages
//! become batches in parallel.

use rustc_hash::FxHashSet;

use bolivar_core::engine::PageTables;
use bolivar_core::layout::{
    Axis, BoxRef, ElementRef, LTChar, LTPage, LineRef, reorder_text_for_output,
};
use bolivar_core::pdfpage::PDFPage;
use bolivar_core::table::{BBox as TableBBox, PageGeometry, TableMetadata};
use bolivar_core::utils::HasBBox;

use crate::abi::FfiResult;
use crate::batch::{
    Builder, Built, NONE, layout, page_text, raw, summary, table_rows, tables, to_i32,
};

// String table slots, so each kind of value keeps its own last-hit cache.
const FONT: usize = 0;
const ORIENTATION: usize = 1;
const MODE: usize = 2;
const TAG: usize = 3;
const NCS: usize = 4;
const SCS: usize = 5;

const NEWLINE: u16 = b'\n' as u16;

fn bbox4(rect: (f64, f64, f64, f64)) -> [f64; 4] {
    rect.into()
}

fn page_number(page_index: usize) -> i64 {
    page_index as i64 + 1
}

/// Sizes for presizing the columns.
#[derive(Default)]
struct Counts {
    lines: usize,
    chars: usize,
    text: usize,
}

fn count(boxes: &[BoxRef<'_>]) -> Counts {
    let mut counts = Counts::default();
    for line in boxes.iter().flat_map(|text_box| text_box.lines()) {
        counts.lines += 1;
        for character in line.elements().filter_map(ElementRef::as_char) {
            counts.chars += 1;
            counts.text += character.get_text().len();
        }
    }
    counts
}

fn orientation(line: LineRef<'_>) -> &'static str {
    match line.axis() {
        Axis::Horizontal => "horizontal",
        Axis::Vertical => "vertical",
    }
}

// ---- the text layer, shared by layout and raw ------------------------------
//
// Raw batches extend layout ones, so `layout::Col` indexes the text layer of
// both.

use layout::Col as T;

fn reserve_text_layer(b: &mut Builder, boxes: usize, counts: &Counts) {
    b.reserve(&[T::BoxBbox.into()], 4 * boxes);
    b.reserve(&[T::BoxMode.into(), T::BoxLines.into()], boxes + 1);
    b.reserve(&[T::BoxText.into()], 2 * boxes);
    b.reserve(&[T::LineBbox.into()], 4 * counts.lines);
    b.reserve(
        &[T::LineOrient.into(), T::LineChars.into()],
        counts.lines + 1,
    );
    b.reserve(&[T::LineText.into()], 2 * counts.lines);
    b.reserve(
        &[
            T::CharX0.into(),
            T::CharY0.into(),
            T::CharX1.into(),
            T::CharY1.into(),
            T::CharSize.into(),
            T::CharFont.into(),
            T::CharUpright.into(),
            T::CharText.into(),
        ],
        counts.chars,
    );
    b.reserve_text(counts.text + boxes + counts.lines);
}

/// Write every box and line. `line` writes a line's text and characters;
/// box texts join line texts and the page text joins boxes with newlines,
/// all as ranges into one run. Returns the end of the page text.
fn write_text_layer(
    b: &mut Builder,
    boxes: &[BoxRef<'_>],
    mut line: impl FnMut(&mut Builder, LineRef<'_>) -> FfiResult<()>,
) -> FfiResult<i32> {
    b.i32s(T::BoxLines).push(0);
    b.i32s(T::LineChars).push(0);
    for (index, text_box) in boxes.iter().enumerate() {
        if index > 0 {
            b.text_unit(NEWLINE);
        }
        let start = b.text_pos()?;
        for text_line in text_box.lines() {
            b.f64s(T::LineBbox).extend(bbox4(text_line.bbox()));
            let orient = b.intern(ORIENTATION, orientation(text_line))?;
            b.i32s(T::LineOrient).push(orient);
            line(b, text_line)?;
            let chars = to_i32(b.i32s(T::CharFont).len(), "chars")?;
            b.i32s(T::LineChars).push(chars);
        }
        let end = b.text_pos()?;
        b.f64s(T::BoxBbox).extend(bbox4(text_box.bbox()));
        let mode = b.intern(MODE, text_box.writing_mode())?;
        b.i32s(T::BoxMode).push(mode);
        b.i32s(T::BoxText).extend([start, end]);
        let lines = to_i32(b.i32s(T::LineOrient).len(), "lines")?;
        b.i32s(T::BoxLines).push(lines);
    }
    b.text_pos()
}

/// Write one character of the text layer. `text` may differ from the
/// glyph's own text: under bidi it is a logical-order span.
fn write_char(b: &mut Builder, text: &str, character: &LTChar) -> FfiResult<()> {
    let (x0, y0, x1, y1) = character.bbox();
    b.f64s(T::CharX0).push(x0);
    b.f64s(T::CharY0).push(y0);
    b.f64s(T::CharX1).push(x1);
    b.f64s(T::CharY1).push(y1);
    b.f64s(T::CharSize).push(character.size());
    let font = b.intern(FONT, character.fontname())?;
    b.i32s(T::CharFont).push(font);
    b.u8s(T::CharUpright).push(u8::from(character.upright()));
    let text = b.intern_glyph(text)?;
    b.i32s(T::CharText).push(text);
    Ok(())
}

// ---- layout ----------------------------------------------------------------

/// The layout page: boxes, lines and characters. Under bidi, characters are
/// the reconstructed spans in logical order.
pub fn layout_built(page_index: usize, page: &LTPage) -> FfiResult<Built> {
    let mut b = Builder::new(&layout::SCHEMA);
    // Borrowed from the compact layout: no character is cloned to be read.
    let boxes = page.text_layer();
    let counts = count(&boxes);
    reserve_text_layer(&mut b, boxes.len(), &counts);
    let end = write_text_layer(&mut b, &boxes, |b, line| {
        if line.bidi() {
            let reconstructed = line.reconstructed();
            let range = b.text(&reconstructed.text)?;
            b.i32s(T::LineText).extend(range);
            for span in &reconstructed.spans {
                if let Some(ElementRef::Char(character)) = line.element(span.source_index) {
                    write_char(b, &span.text, character)?;
                }
            }
        } else {
            let range = b.text(&line.text())?;
            b.i32s(T::LineText).extend(range);
            for character in line.elements().filter_map(ElementRef::as_char) {
                write_char(b, character.get_text(), character)?;
            }
        }
        Ok(())
    })?;
    b.int(layout::Int::PageNumber as usize, page_number(page_index));
    b.int(layout::Int::TextEnd as usize, i64::from(end));
    b.float(layout::Float::Rotate as usize, page.rotate);
    b.float_box(layout::Float::X0 as usize, page.bbox());
    b.finish()
}

// ---- tables ----------------------------------------------------------------

/// The column set of a group of tables.
struct TableColumns {
    table_bbox: usize,
    shape: usize,
    cells: usize,
    index: usize,
    cell_bbox: usize,
    cell_text: usize,
}

/// Write tables and their cells, each box mapped by `bbox`.
fn write_tables(
    b: &mut Builder,
    cols: &TableColumns,
    page_tables: &[TableMetadata],
    bbox: impl Fn(TableBBox) -> [f64; 4],
) -> FfiResult<()> {
    b.i32s(cols.cells).push(0);
    for table in page_tables {
        b.f64s(cols.table_bbox).extend(bbox(table.bbox));
        b.i32s(cols.shape).extend([
            to_i32(table.row_count, "rows")?,
            to_i32(table.column_count, "columns")?,
        ]);
        for cell in &table.cells {
            b.i32s(cols.index).extend([
                to_i32(cell.row_index, "row index")?,
                to_i32(cell.column_index, "column index")?,
                to_i32(cell.row_span, "row span")?,
                to_i32(cell.column_span, "column span")?,
            ]);
            b.f64s(cols.cell_bbox).extend(bbox(cell.bbox));
            let text = b.text(&cell.text)?;
            b.i32s(cols.cell_text).extend(text);
        }
        let cells = to_i32(b.i32s(cols.index).len() / 4, "cells")?;
        b.i32s(cols.cells).push(cells);
    }
    Ok(())
}

fn top_down(bbox: TableBBox) -> [f64; 4] {
    [bbox.x0, bbox.top, bbox.x1, bbox.bottom]
}

// ---- raw -------------------------------------------------------------------

/// Colours repeat character after character, so each one reuses the
/// previous character's pool range when equal (run-length sharing).
#[derive(Default)]
struct ColorRuns {
    last: [Option<[i32; 2]>; 2],
}

impl ColorRuns {
    fn push(
        &mut self,
        b: &mut Builder,
        which: usize,
        col: raw::Col,
        color: Option<&Vec<f64>>,
    ) -> FfiResult<()> {
        let range = match color {
            None => NONE,
            Some(values) => {
                let pool = b.f64s(raw::Col::ColorPool);
                match self.last[which] {
                    // By bit pattern, so -0.0 and 0.0 stay distinct, as the raw page promises.
                    Some([start, end])
                        if pool[start as usize..end as usize]
                            .iter()
                            .map(|v| v.to_bits())
                            .eq(values.iter().map(|v| v.to_bits())) =>
                    {
                        [start, end]
                    }
                    _ => {
                        let start = pool.len();
                        pool.extend_from_slice(values);
                        let range = [
                            to_i32(start, "colors")?,
                            to_i32(start + values.len(), "colors")?,
                        ];
                        self.last[which] = Some(range);
                        range
                    }
                }
            }
        };
        b.i32s(col).extend(range);
        Ok(())
    }
}

/// The lossless page: PDF identity, boxes, lines with raw and output text,
/// every source glyph with its graphics state, and the page's tables in
/// pdfplumber space.
pub fn raw_built(
    page_index: usize,
    pdf_page: &PDFPage,
    page: &LTPage,
    page_tables: &[TableMetadata],
) -> FfiResult<Built> {
    use raw::Col as C;
    let mut b = Builder::new(&raw::SCHEMA);
    let boxes = page.text_layer();
    let counts = count(&boxes);
    reserve_text_layer(&mut b, boxes.len(), &counts);
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
    b.reserve(&[C::LineRawText.into()], 2 * counts.lines);

    let mut colors = ColorRuns::default();
    let end = write_text_layer(&mut b, &boxes, |b, line| {
        // Raw text is content order, annotations included. Without bidi the output text is
        // the raw text reordered for output, so it is built from it, not collected twice.
        let raw_text = line.raw_text();
        let text = if line.bidi() {
            line.text()
        } else {
            reorder_text_for_output(&raw_text)
        };
        let range = b.text(&text)?;
        b.i32s(T::LineText).extend(range);
        if raw_text == text {
            b.i32s(C::LineRawText).extend(range);
        } else {
            b.defer(C::LineRawText, raw_text);
        }
        for character in line.elements().filter_map(ElementRef::as_char) {
            write_char(b, character.get_text(), character)?;
            b.f64s(C::CharAdvance).push(character.adv());
            let m = character.matrix();
            b.f64s(C::CharMatrix).extend([m.0, m.1, m.2, m.3, m.4, m.5]);
            b.i32s(C::CharMcid)
                .push(character.mcid().unwrap_or(i32::MIN));
            let tag = b.intern_opt(TAG, character.tag())?;
            b.i32s(C::CharTag).push(tag);
            let ncs = b.intern_opt(NCS, character.ncs())?;
            b.i32s(C::CharNcs).push(ncs);
            let scs = b.intern_opt(SCS, character.scs())?;
            b.i32s(C::CharScs).push(scs);
            colors.push(b, 0, C::CharNcolor, character.non_stroking_color().as_ref())?;
            colors.push(b, 1, C::CharScolor, character.stroking_color().as_ref())?;
        }
        Ok(())
    })?;
    let label = b.text_opt(pdf_page.label.as_deref())?;
    write_tables(
        &mut b,
        &TableColumns {
            table_bbox: C::TableBboxTopDown.into(),
            shape: C::TableShape.into(),
            cells: C::TableCells.into(),
            index: C::CellIndex.into(),
            cell_bbox: C::CellBboxTopDown.into(),
            cell_text: C::CellText.into(),
        },
        page_tables,
        top_down,
    )?;
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
    b.int(raw::Int::PageIndex as usize, page_index as i64);
    b.int(raw::Int::ObjectId as usize, i64::from(pdf_page.pageid));
    b.int(raw::Int::Rotation as usize, pdf_page.rotate);
    b.int(raw::Int::LabelStart as usize, i64::from(label[0]));
    b.int(raw::Int::LabelEnd as usize, i64::from(label[1]));
    b.int(raw::Int::TextEnd as usize, i64::from(end));
    b.float(raw::Float::UserUnit as usize, pdf_page.user_unit);
    b.float_box(raw::Float::X0 as usize, page.bbox());
    b.finish()
}

// ---- page text -------------------------------------------------------------

/// A page's boxes and lines with and without bidi, and its distinct glyphs
/// as (font, text, size). Java applies its own whitespace, digit and
/// rounding rules to the glyphs, so the summary follows Java semantics
/// exactly.
pub fn page_text_built(page_index: usize, page: &LTPage) -> FfiResult<Built> {
    use page_text::Col as C;
    let mut b = Builder::new(&page_text::SCHEMA);
    let mut glyphs: FxHashSet<(i32, i32, u64)> = FxHashSet::default();
    b.i32s(C::BoxLines).push(0);
    for text_box in page.text_layer() {
        let start = b.text_pos()?;
        // Each line's text without bidi, moved here; joined into the box's only if bidi
        // changed some line, which is rare: without bidi, output and plain text are one string.
        let mut plain_lines: Vec<String> = Vec::new();
        let mut all_equal = true;
        for line in text_box.lines() {
            let without_bidi = line.text_without_bidi();
            b.f64s(C::LineBbox).extend(bbox4(line.bbox()));
            let output = line.bidi().then(|| line.text());
            match output {
                Some(output) if output != without_bidi => {
                    let range = b.text(&output)?;
                    b.i32s(C::LineText).extend(range);
                    all_equal = false;
                    b.defer(C::LineTextWithoutBidi, without_bidi.clone());
                }
                _ => {
                    let range = b.text(&without_bidi)?;
                    b.i32s(C::LineText).extend(range);
                    b.i32s(C::LineTextWithoutBidi).extend(range);
                }
            }
            for character in line.elements().filter_map(ElementRef::as_char) {
                let font = b.intern(FONT, character.fontname())?;
                let text = b.intern_glyph(character.get_text())?;
                if glyphs.insert((font, text, character.size().to_bits())) {
                    b.i32s(C::GlyphFont).push(font);
                    b.i32s(C::GlyphText).push(text);
                    b.f64s(C::GlyphSize).push(character.size());
                }
            }
            plain_lines.push(without_bidi);
        }
        let end = b.text_pos()?;
        b.f64s(C::BoxBbox).extend(bbox4(text_box.bbox()));
        b.i32s(C::BoxText).extend([start, end]);
        if all_equal {
            b.i32s(C::BoxTextWithoutBidi).extend([start, end]);
        } else {
            b.defer(C::BoxTextWithoutBidi, plain_lines.concat());
        }
        let lines = to_i32(b.i32s(C::LineText).len() / 2, "lines")?;
        b.i32s(C::BoxLines).push(lines);
    }
    b.int(page_text::Int::PageNumber as usize, page_number(page_index));
    b.float_box(page_text::Float::X0 as usize, page.bbox());
    b.finish()
}

// ---- summary, tables, table rows ------------------------------------------

/// A page's number, box and rotation; its text is the whole `Text` column.
pub fn summary_built(page_index: usize, page: &LTPage) -> FfiResult<Built> {
    let mut b = Builder::new(&summary::SCHEMA);
    b.text(&page.get_text())?;
    b.int(summary::Int::PageNumber as usize, page_number(page_index));
    b.float(summary::Float::Rotate as usize, page.rotate);
    b.float_box(summary::Float::X0 as usize, page.bbox());
    b.finish()
}

/// The tables of one page, with boxes in PDF space.
pub fn tables_built(
    page_index: usize,
    geometry: &PageGeometry,
    page_tables: &[TableMetadata],
) -> FfiResult<Built> {
    use tables::Col as C;
    let mut b = Builder::new(&tables::SCHEMA);
    write_tables(
        &mut b,
        &TableColumns {
            table_bbox: C::TableBbox.into(),
            shape: C::TableShape.into(),
            cells: C::TableCells.into(),
            index: C::CellIndex.into(),
            cell_bbox: C::CellBbox.into(),
            cell_text: C::CellText.into(),
        },
        page_tables,
        |bbox| bbox4(bbox.to_pdf_space(geometry)),
    )?;
    b.int(tables::Int::PageNumber as usize, page_number(page_index));
    b.finish()
}

/// The raw table rows of one page (absent cells are absent ranges). Row
/// offsets index cells; table offsets index rows; both end with the total.
pub fn table_rows_built(page_index: usize, page_tables: PageTables) -> FfiResult<Built> {
    use table_rows::Col as C;
    let mut b = Builder::new(&table_rows::SCHEMA);
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
    b.int(
        table_rows::Int::PageNumber as usize,
        page_number(page_index),
    );
    b.finish()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::batch::{Column, Kind, common};
    use bolivar_core::layout::{
        LTItem, LTTextBoxHorizontal, LTTextLineHorizontal, TextBoxType, TextLineElement,
    };
    use bolivar_core::pdftypes::PDFDict;
    use bolivar_core::table::TableCellMetadata;

    /// Read a finished batch in tests.
    pub struct View<'a>(pub &'a Built, pub &'a Kind);

    impl View<'_> {
        fn index(&self, name: &str) -> usize {
            self.1
                .columns
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
        pub fn u8s(&self, name: &str) -> &[u8] {
            match &self.0.columns()[self.index(name)] {
                Column::U8(v) => v,
                _ => panic!("{name}"),
            }
        }
        pub fn int(&self, name: &str) -> i64 {
            let slot = self.1.ints.iter().position(|n| *n == name).unwrap();
            match &self.0.columns()[common::INTS] {
                Column::I64(v) => v[slot],
                _ => panic!(),
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
        pub fn strings(&self, name: &str) -> Vec<String> {
            self.i32s(name).iter().map(|&id| self.string(id)).collect()
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

    fn page_of(lines: Vec<LTTextLineHorizontal>) -> LTPage {
        let mut text_box = LTTextBoxHorizontal::new();
        for line in lines {
            text_box.add(line);
        }
        let mut page = LTPage::new(1, (0.0, 0.0, 100.0, 100.0), 0.0);
        page.add(LTItem::TextBox(TextBoxType::Horizontal(text_box)));
        page
    }

    fn test_page(label: Option<&str>) -> PDFPage {
        PDFPage {
            pageid: 42,
            attrs: PDFDict::default(),
            label: label.map(str::to_owned),
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
        }
    }

    #[test]
    fn layout_uses_icu_mapping_when_bidi_is_on() {
        let built = layout_built(0, &page_of(vec![arabic_line(true)])).unwrap();
        let v = View(&built, &layout::SCHEMA);
        assert_eq!(v.ranges("LineText"), vec![Some("العربية\n".to_owned())]);
        assert_eq!(v.strings("CharText").concat(), "العربية");
        assert_eq!(v.int("PageNumber"), 1);
    }

    #[test]
    fn raw_keeps_content_order_beside_output_text() {
        let built = raw_built(0, &test_page(None), &page_of(vec![arabic_line(true)]), &[]).unwrap();
        let v = View(&built, &raw::SCHEMA);
        assert_eq!(v.ranges("LineRawText"), vec![Some("ﺔﻴﺑﺮﻌﻟﺍ\n".to_owned())]);
        assert_eq!(v.ranges("LineText"), vec![Some("العربية\n".to_owned())]);
        assert_eq!(v.strings("CharText")[0], "ﺔ");
    }

    #[test]
    fn raw_shares_the_line_range_when_texts_match() {
        let page = page_of(vec![line(&[("a", "F", 1.0), ("b", "F", 1.0)])]);
        let built = raw_built(0, &test_page(None), &page, &[]).unwrap();
        let v = View(&built, &raw::SCHEMA);
        assert_eq!(v.i32s("LineRawText"), v.i32s("LineText"));
    }

    #[test]
    fn page_text_keeps_both_texts_under_bidi() {
        let without = page_text_built(0, &page_of(vec![arabic_line(false)])).unwrap();
        let with = page_text_built(0, &page_of(vec![arabic_line(true)])).unwrap();
        let (vw, vo) = (
            View(&with, &page_text::SCHEMA),
            View(&without, &page_text::SCHEMA),
        );
        assert_eq!(vw.ranges("LineText")[0].as_deref(), Some("العربية\n"));
        assert_eq!(vw.ranges("LineTextWithoutBidi"), vo.ranges("LineText"));
        assert_eq!(
            vo.i32s("LineText"),
            vo.i32s("LineTextWithoutBidi"),
            "no copy without bidi"
        );
        assert_eq!(vw.ranges("BoxTextWithoutBidi"), vo.ranges("BoxText"));
    }

    #[test]
    fn page_text_ships_distinct_glyphs() {
        let page = page_of(vec![line(&[
            ("A", "F1", 10.0),
            ("A", "F1", 10.0),
            ("7", "F2", 11.4),
        ])]);
        let built = page_text_built(0, &page).unwrap();
        let v = View(&built, &page_text::SCHEMA);
        assert_eq!(v.strings("GlyphFont"), ["F1", "F2"]);
        assert_eq!(v.strings("GlyphText"), ["A", "7"]);
        assert_eq!(v.f64s("GlyphSize"), [10.0, 11.4]);
    }

    #[test]
    fn raw_page_preserves_pdf_identity_boxes_and_table_coordinates() {
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
        let built = raw_built(3, &test_page(Some("iv")), &layout_page, &[table]).unwrap();
        let v = View(&built, &raw::SCHEMA);
        assert_eq!(v.int("PageIndex"), 3);
        assert_eq!(v.int("ObjectId"), 42);
        assert_eq!(v.int("Rotation"), 90);
        assert_eq!(
            v.text(v.int("LabelStart") as i32, v.int("LabelEnd") as i32),
            "iv"
        );
        assert_eq!(v.u8s("PageBoxPresent"), [1, 1, 0, 1, 0]);
        assert_eq!(v.f64s("TableBboxTopDown"), [11.0, 22.0, 99.0, 88.0]);
        assert_eq!(v.ranges("CellText"), vec![Some("value".to_owned())]);
        assert_eq!(v.i32s("TableCells"), [0, 1]);
    }

    #[test]
    fn raw_characters_keep_graphics_state_and_share_colour_runs() {
        let mut text_line = LTTextLineHorizontal::new(0.1);
        for x in 0..3 {
            let x = f64::from(x);
            text_line.add_element(TextLineElement::Char(Box::new(
                LTChar::builder((x, 2.0, x + 1.0, 4.0), "A", "Helvetica", 12.0)
                    .upright(true)
                    .adv(7.5)
                    .matrix((1.0, 0.1, 0.2, 1.0, 20.0, 30.0))
                    .mcid(Some(17))
                    .tag(Some("Span".to_owned()))
                    .ncs(Some("DeviceRGB".to_owned()))
                    .scs(Some("DeviceGray".to_owned()))
                    .non_stroking_color(Some(vec![0.1, 0.2, 0.3]))
                    .stroking_color(Some(vec![0.4]))
                    .build(),
            )));
        }
        let built = raw_built(0, &test_page(None), &page_of(vec![text_line]), &[]).unwrap();
        let v = View(&built, &raw::SCHEMA);
        assert_eq!(v.f64s("CharAdvance"), [7.5, 7.5, 7.5]);
        assert_eq!(&v.f64s("CharMatrix")[..6], [1.0, 0.1, 0.2, 1.0, 20.0, 30.0]);
        assert_eq!(v.i32s("CharMcid"), [17, 17, 17]);
        assert_eq!(v.string(v.i32s("CharTag")[0]), "Span");
        assert_eq!(v.string(v.i32s("CharNcs")[0]), "DeviceRGB");
        assert_eq!(v.string(v.i32s("CharScs")[0]), "DeviceGray");
        assert_eq!(
            v.f64s("ColorPool"),
            [0.1, 0.2, 0.3, 0.4],
            "one copy for three characters"
        );
        assert_eq!(v.i32s("CharNcolor"), [0, 3, 0, 3, 0, 3]);
        assert_eq!(v.i32s("CharScolor"), [3, 4, 3, 4, 3, 4]);
    }

    #[test]
    fn table_rows_flatten_tables_with_empty_cells() {
        let built = table_rows_built(
            2,
            vec![
                vec![vec![Some("a".to_owned()), None], Vec::new()],
                vec![vec![None, Some("b".to_owned())]],
            ],
        )
        .unwrap();
        let v = View(&built, &table_rows::SCHEMA);
        assert_eq!(v.int("PageNumber"), 3);
        assert_eq!(
            v.ranges("CellText"),
            vec![Some("a".to_owned()), None, None, Some("b".to_owned())]
        );
        assert_eq!(v.i32s("RowOffsets"), [0, 2, 2, 4]);
        assert_eq!(v.i32s("TableOffsets"), [0, 2, 3]);
        let empty = table_rows_built(0, vec![Vec::new(), vec![Vec::new()]]).unwrap();
        let v = View(&empty, &table_rows::SCHEMA);
        assert!(v.i32s("CellText").is_empty());
        assert_eq!(v.i32s("RowOffsets"), [0, 0]);
        assert_eq!(v.i32s("TableOffsets"), [0, 0, 1]);
    }
}
