package sa.ingenious.pdf;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import sa.ingenious.pdf.internal.Columns;
import sa.ingenious.pdf.internal.Schema;

/** Turns the small batch kinds into records. */
final class Decode {
  private Decode() {}

  static String text(Columns c) {
    return c.text();
  }

  static PageSummary summary(Columns c) {
    long[] ints = c.i64(Schema.Summary.Ints);
    double[] floats = c.f64(Schema.Summary.Floats);
    return new PageSummary(
        Math.toIntExact(ints[0]),
        c.range((int) ints[1], (int) ints[2]),
        new BoundingBox(floats[1], floats[2], floats[3], floats[4]),
        floats[0]);
  }

  static PageText pageText(Columns c) {
    double[] boxBbox = c.f64(Schema.PageText.BoxBbox);
    int[] boxLines = c.i32(Schema.PageText.BoxLines);
    double[] lineBbox = c.f64(Schema.PageText.LineBbox);
    List<PageTextBox> boxes = new ArrayList<>(boxLines.length - 1);
    for (int b = 0; b < boxLines.length - 1; b++) {
      List<PageTextLine> lines = new ArrayList<>(boxLines[b + 1] - boxLines[b]);
      for (int l = boxLines[b]; l < boxLines[b + 1]; l++) {
        lines.add(
            new PageTextLine(
                LayoutPage.box(lineBbox, l),
                c.range(Schema.PageText.LineText, l),
                c.range(Schema.PageText.LineTextWithoutBidi, l)));
      }
      boxes.add(
          new PageTextBox(
              LayoutPage.box(boxBbox, b),
              c.range(Schema.PageText.BoxText, b),
              c.range(Schema.PageText.BoxTextWithoutBidi, b),
              List.copyOf(lines)));
    }
    List<String> fonts =
        Arrays.stream(c.i32(Schema.PageText.Fonts)).mapToObj(c::string).toList();
    int[] digitFont = c.i32(Schema.PageText.DigitFont);
    int[] digitSize = c.i32(Schema.PageText.DigitSize);
    List<DigitStyle> digits = new ArrayList<>(digitFont.length);
    for (int i = 0; i < digitFont.length; i++) {
      digits.add(new DigitStyle(c.string(digitFont[i]), digitSize[i]));
    }
    double[] floats = c.f64(Schema.PageText.Floats);
    return new PageText(
        Math.toIntExact(c.i64(Schema.PageText.Ints)[0]),
        new BoundingBox(floats[0], floats[1], floats[2], floats[3]),
        List.copyOf(boxes),
        fonts,
        List.copyOf(digits));
  }

  static List<Table> tables(Columns c) {
    int pageNumber = Math.toIntExact(c.i64(Schema.Tables.Ints)[0]);
    double[] tableBbox = c.f64(Schema.Tables.TableBbox);
    int[] shape = c.i32(Schema.Tables.TableShape);
    int[] cellOffsets = c.i32(Schema.Tables.TableCells);
    int[] index = c.i32(Schema.Tables.CellIndex);
    double[] cellBbox = c.f64(Schema.Tables.CellBbox);
    List<Table> tables = new ArrayList<>(shape.length / 2);
    for (int t = 0; t < shape.length / 2; t++) {
      List<TableCell> cells = new ArrayList<>(cellOffsets[t + 1] - cellOffsets[t]);
      for (int i = cellOffsets[t]; i < cellOffsets[t + 1]; i++) {
        cells.add(
            new TableCell(
                index[4 * i],
                index[4 * i + 1],
                index[4 * i + 2],
                index[4 * i + 3],
                LayoutPage.box(cellBbox, i),
                c.range(Schema.Tables.CellText, i)));
      }
      tables.add(
          new Table(
              pageNumber,
              LayoutPage.box(tableBbox, t),
              shape[2 * t],
              shape[2 * t + 1],
              List.copyOf(cells)));
    }
    return tables;
  }

  static PageTableRows tableRows(Columns c) {
    int[] ranges = c.i32(Schema.TableRows.CellText);
    String[] cells = new String[ranges.length / 2];
    for (int i = 0; i < cells.length; i++) {
      cells[i] = c.range(ranges[2 * i], ranges[2 * i + 1]);
    }
    return new PageTableRows(
        Math.toIntExact(c.i64(Schema.TableRows.Ints)[0]),
        cells,
        c.i32(Schema.TableRows.RowOffsets),
        c.i32(Schema.TableRows.TableOffsets));
  }

  // Flag bits of the metadata batch, as `metadata::flags`.
  private static boolean bit(long flags, int bit) {
    return (flags & (1L << bit)) != 0;
  }

  static Metadata metadata(Columns c) {
    long[] ints = c.i64(Schema.Metadata.Ints);
    String[] f = new String[15];
    for (int i = 0; i < f.length; i++) {
      f[i] = c.range(Schema.Metadata.Fields, i);
    }
    int entries = c.i32(Schema.Metadata.InfoKeys).length / 2;
    List<MetadataEntry> info = new ArrayList<>(entries);
    for (int i = 0; i < entries; i++) {
      info.add(
          new MetadataEntry(
              c.range(Schema.Metadata.InfoKeys, i), c.range(Schema.Metadata.InfoValues, i)));
    }
    long flags = ints[2];
    return new Metadata(
        List.copyOf(info),
        f[0],
        f[1],
        f[2],
        f[3],
        f[4],
        f[5],
        f[6],
        f[7],
        f[8],
        f[9],
        new PdfVersion(f[10], f[11], f[12]),
        ints[0],
        Math.toIntExact(ints[1]),
        bit(flags, 0),
        new PdfPermissions(bit(flags, 1), bit(flags, 2), bit(flags, 3)),
        bit(flags, 4),
        bit(flags, 5),
        bit(flags, 6),
        bit(flags, 7),
        f[13],
        bit(flags, 8),
        bit(flags, 9),
        f[14]);
  }
}
