package sa.ingenious.pdf;

import java.nio.charset.StandardCharsets;
import java.util.Comparator;
import java.util.List;
import java.util.TreeSet;
import java.util.function.Consumer;
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

  static PageSummary summary(Columns c) {
    char[] text = c.text();
    return new PageSummary(
        Math.toIntExact(c.integer(Schema.Summary.IntSlot.PageNumber)),
        new String(text),
        new BoundingBox(
            c.real(Schema.Summary.FloatSlot.X0),
            c.real(Schema.Summary.FloatSlot.Y0),
            c.real(Schema.Summary.FloatSlot.X1),
            c.real(Schema.Summary.FloatSlot.Y1)),
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
        lines[l] =
            new PageTextLine(
                TextLayer.box(lineBbox, line),
                range(c, Schema.PageText.Col.LineText, line),
                range(c, Schema.PageText.Col.LineTextWithoutBidi, line));
      }
      boxes[b] =
          new PageTextBox(
              TextLayer.box(boxBbox, b),
              range(c, Schema.PageText.Col.BoxText, b),
              range(c, Schema.PageText.Col.BoxTextWithoutBidi, b),
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
    for (int g = 0; g < glyphFont.length; g++) {
      String text = strings[glyphText[g]];
      if (text.isBlank()) {
        continue;
      }
      String font = strings[glyphFont[g]];
      fonts.add(font);
      if (hasDecimalDigit(text)) {
        digits.add(new DigitStyle(font, wholeSize(glyphSize[g])));
      }
    }
    return new PageText(
        Math.toIntExact(c.integer(Schema.PageText.IntSlot.PageNumber)),
        new BoundingBox(
            c.real(Schema.PageText.FloatSlot.X0),
            c.real(Schema.PageText.FloatSlot.Y0),
            c.real(Schema.PageText.FloatSlot.X1),
            c.real(Schema.PageText.FloatSlot.Y1)),
        TextLayer.wrap(boxes),
        List.copyOf(fonts),
        List.copyOf(digits));
  }

  // ---- tables -----------------------------------------------------------------------------

  /** Each table of one page batch, in order. */
  static void tables(Columns c, Consumer<? super Table> out) {
    int pageNumber = Math.toIntExact(c.integer(Schema.Tables.IntSlot.PageNumber));
    double[] tableBbox = c.f64(Schema.Tables.Col.TableBbox);
    int[] shape = c.i32(Schema.Tables.Col.TableShape);
    int[] cellOffsets = c.i32(Schema.Tables.Col.TableCells);
    int[] index = c.i32(Schema.Tables.Col.CellIndex);
    double[] cellBbox = c.f64(Schema.Tables.Col.CellBbox);
    for (int t = 0; t < shape.length / 2; t++) {
      TableCell[] cells = new TableCell[cellOffsets[t + 1] - cellOffsets[t]];
      for (int k = 0; k < cells.length; k++) {
        int i = cellOffsets[t] + k;
        cells[k] =
            new TableCell(
                index[4 * i],
                index[4 * i + 1],
                index[4 * i + 2],
                index[4 * i + 3],
                TextLayer.box(cellBbox, i),
                range(c, Schema.Tables.Col.CellText, i));
      }
      out.accept(
          new Table(
              pageNumber,
              TextLayer.box(tableBbox, t),
              shape[2 * t],
              shape[2 * t + 1],
              TextLayer.wrap(cells)));
    }
  }

  static PageTableRows tableRows(Columns c) {
    int[] ranges = c.i32(Schema.TableRows.Col.CellText);
    @Nullable String[] cells = new String[ranges.length / 2];
    for (int i = 0; i < cells.length; i++) {
      cells[i] = c.range(ranges[2 * i], ranges[2 * i + 1]);
    }
    return new PageTableRows(
        Math.toIntExact(c.integer(Schema.TableRows.IntSlot.PageNumber)),
        cells,
        c.i32(Schema.TableRows.Col.RowOffsets),
        c.i32(Schema.TableRows.Col.TableOffsets));
  }

  // ---- metadata ---------------------------------------------------------------------------

  static Metadata metadata(Columns c) {
    RangeSlot[] slots = RangeSlot.values();
    @Nullable String[] f = new String[slots.length];
    for (RangeSlot slot : slots) {
      f[slot.ordinal()] = c.range(Schema.Metadata.Col.Ranges, slot.ordinal());
    }
    int entries = c.i32(Schema.Metadata.Col.InfoKeys).length / 2;
    MetadataEntry[] info = new MetadataEntry[entries];
    for (int i = 0; i < entries; i++) {
      info[i] =
          new MetadataEntry(
              range(c, Schema.Metadata.Col.InfoKeys, i),
              range(c, Schema.Metadata.Col.InfoValues, i));
    }
    long flags = c.integer(Schema.Metadata.IntSlot.Flags);
    String header = f[RangeSlot.VersionHeader.ordinal()];
    String catalog = f[RangeSlot.VersionCatalog.ordinal()];
    String xmp = f[RangeSlot.Xmp.ordinal()];
    String form = f[RangeSlot.Form.ordinal()];
    return new Metadata(
        TextLayer.wrap(info),
        f[RangeSlot.Title.ordinal()],
        f[RangeSlot.Author.ordinal()],
        f[RangeSlot.Subject.ordinal()],
        f[RangeSlot.Keywords.ordinal()],
        f[RangeSlot.Creator.ordinal()],
        f[RangeSlot.Producer.ordinal()],
        f[RangeSlot.CreationDateRaw.ordinal()],
        f[RangeSlot.CreationDateIso.ordinal()],
        f[RangeSlot.ModificationDateRaw.ordinal()],
        f[RangeSlot.ModificationDateIso.ordinal()],
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
        form == null ? "none" : form,
        FlagBit.HasJavascript.in(flags),
        xmp != null,
        xmp);
  }
}
