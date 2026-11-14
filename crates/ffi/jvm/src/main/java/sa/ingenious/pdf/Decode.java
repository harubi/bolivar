package sa.ingenious.pdf;

import java.nio.charset.StandardCharsets;
import java.util.Comparator;
import java.util.HashSet;
import java.util.List;
import java.util.TreeSet;
import java.util.function.Consumer;
import java.util.function.IntFunction;
import org.jspecify.annotations.Nullable;
import sa.ingenious.pdf.internal.Columns;
import sa.ingenious.pdf.internal.Schema;
import sa.ingenious.pdf.internal.Schema.Metadata.FlagBit;
import sa.ingenious.pdf.internal.Schema.Metadata.RangeSlot;

/** Turns the small batch kinds into records. */
// Metadata range ordinals index the native Ranges column; Schema.verify checks the order.
@SuppressWarnings("EnumOrdinal")
final class Decode {
  private Decode() {}

  static String text(Columns c) {
    return new String(c.u8(Schema.Text.Col.Utf8), StandardCharsets.UTF_8);
  }

  private static String range(Columns c, Schema.Col column, int index) {
    String text = c.range(column, index);
    if (text == null) {
      throw new IllegalStateException(column + " " + index + " is absent");
    }
    return text;
  }

  /** The text without bidi: the output text itself when native code gave both one range. */
  private static String withoutBidi(
      Columns c, Schema.Col output, Schema.Col plain, int index, String outputText) {
    return TextLayer.sameRange(c.i32(output), c.i32(plain), index)
        ? outputText
        : range(c, plain, index);
  }

  static PageSummary summary(Columns c) {
    char[] text = c.text();
    return new PageSummary(
        Math.toIntExact(c.integer(Schema.Summary.IntSlot.PageNumber)),
        new String(text),
        c.box(Schema.Summary.FloatSlot.X0),
        c.real(Schema.Summary.FloatSlot.Rotate));
  }

  // ---- page text --------------------------------------------------------------------------

  /** Strings in code point order, as Rust's {@code BTreeSet<String>} sorts them. */
  static final Comparator<String> CODE_POINT_ORDER =
      (a, b) -> {
        int i = 0;
        int j = 0;
        while (i < a.length() && j < b.length()) {
          int x = a.codePointAt(i);
          int y = b.codePointAt(j);
          if (x != y) {
            return Integer.compare(x, y);
          }
          i += Character.charCount(x);
          j += Character.charCount(y);
        }
        return Integer.compare(a.length() - i, b.length() - j);
      };

  private static final Comparator<DigitStyle> DIGIT_ORDER =
      Comparator.comparing(DigitStyle::fontName, CODE_POINT_ORDER)
          .thenComparingInt(DigitStyle::size);

  /** Whether {@code text} holds a decimal digit (Unicode category Nd). */
  static boolean hasDecimalDigit(String text) {
    for (int i = 0; i < text.length(); ) {
      int cp = text.codePointAt(i);
      if (Character.getType(cp) == Character.DECIMAL_DIGIT_NUMBER) {
        return true;
      }
      i += Character.charCount(cp);
    }
    return false;
  }

  /** A glyph size as a whole number: {@link Math#round(double)}, saturated to the int range. */
  static int wholeSize(double size) {
    long rounded = Math.round(size);
    return (int) Math.max(Integer.MIN_VALUE, Math.min(Integer.MAX_VALUE, rounded));
  }

  static PageText pageText(Columns c) {
    double[] boxBbox = c.f64(Schema.PageText.Col.BoxBbox);
    int[] boxLines = c.i32(Schema.PageText.Col.BoxLines);
    double[] lineBbox = c.f64(Schema.PageText.Col.LineBbox);
    PageTextBox[] boxes = new PageTextBox[Math.max(boxLines.length - 1, 0)];
    for (int b = 0; b < boxes.length; b++) {
      PageTextLine[] lines = new PageTextLine[boxLines[b + 1] - boxLines[b]];
      for (int l = 0; l < lines.length; l++) {
        int line = boxLines[b] + l;
        String text = range(c, Schema.PageText.Col.LineText, line);
        lines[l] =
            new PageTextLine(
                TextLayer.box(lineBbox, line),
                text,
                withoutBidi(
                    c,
                    Schema.PageText.Col.LineText,
                    Schema.PageText.Col.LineTextWithoutBidi,
                    line,
                    text));
      }
      String text = range(c, Schema.PageText.Col.BoxText, b);
      boxes[b] =
          new PageTextBox(
              TextLayer.box(boxBbox, b),
              text,
              withoutBidi(
                  c, Schema.PageText.Col.BoxText, Schema.PageText.Col.BoxTextWithoutBidi, b, text),
              TextLayer.wrap(lines));
    }
    // Fonts of every glyph whose text is not blank, and the font and whole size of every such
    // glyph holding a decimal digit. Native code ships each distinct (font, text, size) once.
    String[] strings = c.strings();
    int[] glyphFont = c.i32(Schema.PageText.Col.GlyphFont);
    int[] glyphText = c.i32(Schema.PageText.Col.GlyphText);
    double[] glyphSize = c.f64(Schema.PageText.Col.GlyphSize);
    TreeSet<String> fonts = new TreeSet<>(CODE_POINT_ORDER);
    TreeSet<DigitStyle> digits = new TreeSet<>(DIGIT_ORDER);
    // A page has few fonts: sort each once, keyed by its string id, not once per glyph.
    boolean[] seenFont = new boolean[strings.length];
    HashSet<Long> seenDigit = new HashSet<>();
    for (int g = 0; g < glyphFont.length; g++) {
      String text = strings[glyphText[g]];
      if (text.isBlank()) {
        continue;
      }
      int font = glyphFont[g];
      if (!seenFont[font]) {
        seenFont[font] = true;
        fonts.add(strings[font]);
      }
      if (hasDecimalDigit(text)) {
        int size = wholeSize(glyphSize[g]);
        if (seenDigit.add(((long) font << 32) | (size & 0xFFFF_FFFFL))) {
          digits.add(new DigitStyle(strings[font], size));
        }
      }
    }
    return new PageText(
        Math.toIntExact(c.integer(Schema.PageText.IntSlot.PageNumber)),
        c.box(Schema.PageText.FloatSlot.X0),
        TextLayer.wrap(boxes),
        List.copyOf(fonts),
        List.copyOf(digits));
  }

  // ---- tables -----------------------------------------------------------------------------

  /** Makes the {@code i}th cell of a page. */
  @FunctionalInterface
  interface CellMaker<C> {
    C cell(int i, int row, int column, int rowSpan, int columnSpan);
  }

  /** Makes the {@code t}th table of a page from its cells. */
  @FunctionalInterface
  interface TableMaker<C, T> {
    T table(int t, int rows, int columns, List<C> cells);
  }

  /**
   * Walk a page's table columns, as native code writes them for table and raw batches alike: (rows,
   * columns) per table, cell offsets per table, and four index entries per cell.
   */
  static <C, T> void tables(
      int[] shape,
      int[] cellOffsets,
      int[] index,
      IntFunction<C[]> newCells,
      CellMaker<C> cell,
      TableMaker<C, T> table,
      Consumer<? super T> out) {
    for (int t = 0; t < shape.length / 2; t++) {
      C[] cells = newCells.apply(cellOffsets[t + 1] - cellOffsets[t]);
      for (int k = 0; k < cells.length; k++) {
        int i = cellOffsets[t] + k;
        cells[k] = cell.cell(i, index[4 * i], index[4 * i + 1], index[4 * i + 2], index[4 * i + 3]);
      }
      out.accept(table.table(t, shape[2 * t], shape[2 * t + 1], TextLayer.wrap(cells)));
    }
  }

  /** Each table of one page batch, in order. */
  static void tables(Columns c, Consumer<? super Table> out) {
    int pageNumber = Math.toIntExact(c.integer(Schema.Tables.IntSlot.PageNumber));
    double[] tableBbox = c.f64(Schema.Tables.Col.TableBbox);
    double[] cellBbox = c.f64(Schema.Tables.Col.CellBbox);
    tables(
        c.i32(Schema.Tables.Col.TableShape),
        c.i32(Schema.Tables.Col.TableCells),
        c.i32(Schema.Tables.Col.CellIndex),
        TableCell[]::new,
        (i, row, column, rowSpan, columnSpan) ->
            new TableCell(
                row,
                column,
                rowSpan,
                columnSpan,
                TextLayer.box(cellBbox, i),
                range(c, Schema.Tables.Col.CellText, i)),
        (t, rows, columns, cells) ->
            new Table(pageNumber, TextLayer.box(tableBbox, t), rows, columns, cells),
        out);
  }

  static PageTableRows tableRows(Columns c) {
    @Nullable String[] cells = new String[c.i32(Schema.TableRows.Col.CellText).length / 2];
    for (int i = 0; i < cells.length; i++) {
      cells[i] = c.range(Schema.TableRows.Col.CellText, i);
    }
    return new PageTableRows(
        Math.toIntExact(c.integer(Schema.TableRows.IntSlot.PageNumber)),
        cells,
        c.i32(Schema.TableRows.Col.RowOffsets),
        c.i32(Schema.TableRows.Col.TableOffsets));
  }

  // ---- metadata ---------------------------------------------------------------------------

  static Metadata metadata(Columns c) {
    int entries = c.i32(Schema.Metadata.Col.InfoKeys).length / 2;
    MetadataEntry[] info = new MetadataEntry[entries];
    for (int i = 0; i < entries; i++) {
      info[i] =
          new MetadataEntry(
              range(c, Schema.Metadata.Col.InfoKeys, i),
              range(c, Schema.Metadata.Col.InfoValues, i));
    }
    long flags = c.integer(Schema.Metadata.IntSlot.Flags);
    String header = field(c, RangeSlot.VersionHeader);
    String catalog = field(c, RangeSlot.VersionCatalog);
    String xmp = field(c, RangeSlot.Xmp);
    return new Metadata(
        TextLayer.wrap(info),
        field(c, RangeSlot.Title),
        field(c, RangeSlot.Author),
        field(c, RangeSlot.Subject),
        field(c, RangeSlot.Keywords),
        field(c, RangeSlot.Creator),
        field(c, RangeSlot.Producer),
        field(c, RangeSlot.CreationDateRaw),
        field(c, RangeSlot.CreationDateIso),
        field(c, RangeSlot.ModificationDateRaw),
        field(c, RangeSlot.ModificationDateIso),
        new PdfVersion(header, catalog, catalog != null ? catalog : header),
        c.integer(Schema.Metadata.IntSlot.FileSize),
        Math.toIntExact(c.integer(Schema.Metadata.IntSlot.PageCount)),
        FlagBit.Encrypted.in(flags),
        new PdfPermissions(
            FlagBit.Printable.in(flags),
            FlagBit.Modifiable.in(flags),
            FlagBit.Extractable.in(flags)),
        FlagBit.Linearized.in(flags),
        FlagBit.Tagged.in(flags),
        FlagBit.UserProperties.in(flags),
        FlagBit.Suspects.in(flags),
        // Native code always writes the form type.
        range(c, Schema.Metadata.Col.Ranges, RangeSlot.Form.ordinal()),
        FlagBit.HasJavascript.in(flags),
        xmp != null,
        xmp);
  }

  /** One named metadata range, or null when absent. */
  private static @Nullable String field(Columns c, RangeSlot slot) {
    return c.range(Schema.Metadata.Col.Ranges, slot.ordinal());
  }
}
