package sa.ingenious.pdf;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import sa.ingenious.pdf.internal.Columns;
import sa.ingenious.pdf.internal.Schema.Raw;

/**
 * One page with nothing dropped: PDF identity and boxes, text boxes and lines with raw and output
 * text, every source glyph with its graphics state, and the page's tables in pdfplumber space.
 *
 * <p>Indexed accessors read the columns directly. {@link #characterAt}, {@link #textBoxes}
 * and {@link #tables} copy, so what they return does not keep the page alive.
 */
public final class RawPage {
  private final int pageIndex;
  private final int pageNumber;
  private final long objectId;
  private final String label;
  private final long rotation;
  private final double userUnit;
  private final RawPageBoxes boxes;
  private final BoundingBox layoutBbox;
  private final String text;
  private final String allText;
  private final String[] strings;
  private final double[] boxBbox;
  private final int[] boxMode;
  private final int[] boxText;
  private final int[] boxLines;
  private final double[] lineBbox;
  private final int[] lineOrient;
  private final int[] lineRawText;
  private final int[] lineText;
  private final int[] lineChars;
  private final double[] x0;
  private final double[] y0;
  private final double[] x1;
  private final double[] y1;
  private final double[] size;
  private final int[] font;
  private final byte[] upright;
  private final int[] charText;
  private final double[] advance;
  private final double[] matrix;
  private final int[] mcid;
  private final int[] tag;
  private final int[] ncs;
  private final int[] scs;
  private final int[] ncolor;
  private final int[] scolor;
  private final double[] colorPool;
  private final List<RawTable> tables;

  RawPage(Columns c) {
    long[] ints = c.i64(Raw.Ints);
    double[] floats = c.f64(Raw.Floats);
    pageIndex = Math.toIntExact(ints[0]);
    pageNumber = Math.toIntExact(ints[1]);
    objectId = ints[2];
    rotation = ints[3];
    label = c.range((int) ints[4], (int) ints[5]);
    text = c.range((int) ints[6], (int) ints[7]);
    userUnit = floats[0];
    layoutBbox = new BoundingBox(floats[1], floats[2], floats[3], floats[4]);
    boxes = pageBoxes(c.f64(Raw.PageBoxes), c.u8(Raw.PageBoxPresent));
    allText = c.text();
    strings = c.strings();
    boxBbox = c.f64(Raw.BoxBbox);
    boxMode = c.i32(Raw.BoxMode);
    boxText = c.i32(Raw.BoxText);
    boxLines = c.i32(Raw.BoxLines);
    lineBbox = c.f64(Raw.LineBbox);
    lineOrient = c.i32(Raw.LineOrient);
    lineRawText = c.i32(Raw.LineRawText);
    lineText = c.i32(Raw.LineText);
    lineChars = c.i32(Raw.LineChars);
    x0 = c.f64(Raw.CharX0);
    y0 = c.f64(Raw.CharY0);
    x1 = c.f64(Raw.CharX1);
    y1 = c.f64(Raw.CharY1);
    size = c.f64(Raw.CharSize);
    font = c.i32(Raw.CharFont);
    upright = c.u8(Raw.CharUpright);
    charText = c.i32(Raw.CharText);
    advance = c.f64(Raw.CharAdvance);
    matrix = c.f64(Raw.CharMatrix);
    mcid = c.i32(Raw.CharMcid);
    tag = c.i32(Raw.CharTag);
    ncs = c.i32(Raw.CharNcs);
    scs = c.i32(Raw.CharScs);
    ncolor = c.i32(Raw.CharNcolor);
    scolor = c.i32(Raw.CharScolor);
    colorPool = c.f64(Raw.ColorPool);
    tables = rawTables(c);
  }

  private static RawPageBoxes pageBoxes(double[] values, byte[] present) {
    List<List<Double>> boxes = new ArrayList<>(5);
    int offset = 0;
    for (byte flag : present) {
      if (flag != 0) {
        boxes.add(
            List.of(values[offset], values[offset + 1], values[offset + 2], values[offset + 3]));
        offset += 4;
      } else {
        boxes.add(null);
      }
    }
    return new RawPageBoxes(boxes.get(0), boxes.get(1), boxes.get(2), boxes.get(3), boxes.get(4));
  }

  private static RawTableBoundingBox tableBox(double[] boxes, int index) {
    int o = 4 * index;
    return new RawTableBoundingBox(boxes[o], boxes[o + 1], boxes[o + 2], boxes[o + 3]);
  }

  private static List<RawTable> rawTables(Columns c) {
    double[] tableBbox = c.f64(Raw.TableBbox);
    int[] shape = c.i32(Raw.TableShape);
    int[] cellOffsets = c.i32(Raw.TableCells);
    int[] index = c.i32(Raw.CellIndex);
    double[] cellBbox = c.f64(Raw.CellBbox);
    List<RawTable> tables = new ArrayList<>(shape.length / 2);
    for (int t = 0; t < shape.length / 2; t++) {
      List<RawTableCell> cells = new ArrayList<>();
      for (int i = cellOffsets[t]; i < cellOffsets[t + 1]; i++) {
        cells.add(
            new RawTableCell(
                index[4 * i],
                index[4 * i + 1],
                index[4 * i + 2],
                index[4 * i + 3],
                tableBox(cellBbox, i),
                c.range(Raw.CellText, i)));
      }
      tables.add(
          new RawTable(tableBox(tableBbox, t), shape[2 * t], shape[2 * t + 1], List.copyOf(cells)));
    }
    return List.copyOf(tables);
  }

  private String range(int[] ranges, int index) {
    int start = ranges[2 * index];
    return start < 0 ? null : allText.substring(start, ranges[2 * index + 1]);
  }

  private List<Double> colors(int[] ranges, int i) {
    int start = ranges[2 * i];
    if (start < 0) {
      return null;
    }
    int end = ranges[2 * i + 1];
    Double[] values = new Double[end - start];
    for (int k = start; k < end; k++) {
      values[k - start] = colorPool[k];
    }
    return List.of(values);
  }

  /** The 0-based index of the page in the document. */
  public int pageIndex() {
    return pageIndex;
  }

  public int pageNumber() {
    return pageNumber;
  }

  /** The page object's number in the PDF. */
  public long objectId() {
    return objectId;
  }

  /** The page label (e.g. {@code iv}), or null. */
  public String label() {
    return label;
  }

  public long rotation() {
    return rotation;
  }

  public double userUnit() {
    return userUnit;
  }

  public RawPageBoxes boxes() {
    return boxes;
  }

  public BoundingBox layoutBbox() {
    return layoutBbox;
  }

  public String text() {
    return text;
  }

  public List<RawTable> tables() {
    return tables;
  }

  // ---- text boxes and lines -------------------------------------------------------------------

  public int textBoxCount() {
    return boxMode.length;
  }

  public BoundingBox textBoxBbox(int box) {
    return LayoutPage.box(boxBbox, box);
  }

  public String textBoxWritingMode(int box) {
    return strings[boxMode[box]];
  }

  public String textBoxText(int box) {
    return range(boxText, box);
  }

  public int textBoxLineStart(int box) {
    return boxLines[box];
  }

  public int textBoxLineEnd(int box) {
    return boxLines[box + 1];
  }

  public int lineCount() {
    return lineOrient.length;
  }

  public BoundingBox lineBbox(int line) {
    return LayoutPage.box(lineBbox, line);
  }

  public String lineOrientation(int line) {
    return strings[lineOrient[line]];
  }

  /** The line's text in content order, before bidi. */
  public String lineRawText(int line) {
    return range(lineRawText, line);
  }

  public String lineText(int line) {
    return range(lineText, line);
  }

  public int lineCharStart(int line) {
    return lineChars[line];
  }

  public int lineCharEnd(int line) {
    return lineChars[line + 1];
  }

  // ---- characters ---------------------------------------------------------------------------

  public int charCount() {
    return font.length;
  }

  public String charText(int i) {
    return range(charText, i);
  }

  public double charX0(int i) {
    return x0[i];
  }

  public double charY0(int i) {
    return y0[i];
  }

  public double charX1(int i) {
    return x1[i];
  }

  public double charY1(int i) {
    return y1[i];
  }

  public BoundingBox charBbox(int i) {
    return new BoundingBox(x0[i], y0[i], x1[i], y1[i]);
  }

  public double charSize(int i) {
    return size[i];
  }

  public String charFontName(int i) {
    return strings[font[i]];
  }

  public boolean charUpright(int i) {
    return upright[i] != 0;
  }

  public double charAdvance(int i) {
    return advance[i];
  }

  /** Entry {@code k} (0 to 5) of the character's text matrix. */
  public double charMatrix(int i, int k) {
    return matrix[6 * i + k];
  }

  /** The marked content id, or null. */
  public Integer charMarkedContentId(int i) {
    return mcid[i] == Integer.MIN_VALUE ? null : mcid[i];
  }

  public String charTag(int i) {
    return tag[i] < 0 ? null : strings[tag[i]];
  }

  public String charNonStrokingColorSpace(int i) {
    return ncs[i] < 0 ? null : strings[ncs[i]];
  }

  public String charStrokingColorSpace(int i) {
    return scs[i] < 0 ? null : strings[scs[i]];
  }

  public List<Double> charNonStrokingColor(int i) {
    return colors(ncolor, i);
  }

  public List<Double> charStrokingColor(int i) {
    return colors(scolor, i);
  }

  public RawCharacter characterAt(int i) {
    int m = 6 * i;
    return new RawCharacter(
        charText(i),
        charBbox(i),
        charFontName(i),
        charSize(i),
        charUpright(i),
        charAdvance(i),
        List.of(matrix[m], matrix[m + 1], matrix[m + 2], matrix[m + 3], matrix[m + 4], matrix[m + 5]),
        charMarkedContentId(i),
        charTag(i),
        charNonStrokingColorSpace(i),
        charStrokingColorSpace(i),
        charNonStrokingColor(i),
        charStrokingColor(i));
  }

  // ---- whole columns (copies) ---------------------------------------------------------------

  public double[] charX0s() {
    return x0.clone();
  }

  public double[] charY0s() {
    return y0.clone();
  }

  public double[] charX1s() {
    return x1.clone();
  }

  public double[] charY1s() {
    return y1.clone();
  }

  public double[] charSizes() {
    return size.clone();
  }

  public double[] charAdvances() {
    return advance.clone();
  }

  public String[] charFontNames() {
    String[] names = new String[font.length];
    Arrays.setAll(names, i -> strings[font[i]]);
    return names;
  }

  // ---- records --------------------------------------------------------------------------------

  public List<RawTextBox> textBoxes() {
    List<RawTextBox> result = new ArrayList<>(textBoxCount());
    for (int b = 0; b < textBoxCount(); b++) {
      List<RawTextLine> lines = new ArrayList<>(textBoxLineEnd(b) - textBoxLineStart(b));
      for (int l = textBoxLineStart(b); l < textBoxLineEnd(b); l++) {
        List<RawCharacter> chars = new ArrayList<>(lineCharEnd(l) - lineCharStart(l));
        for (int i = lineCharStart(l); i < lineCharEnd(l); i++) {
          chars.add(characterAt(i));
        }
        lines.add(
            new RawTextLine(
                lineBbox(l), lineOrientation(l), lineRawText(l), lineText(l), List.copyOf(chars)));
      }
      result.add(
          new RawTextBox(textBoxBbox(b), textBoxWritingMode(b), textBoxText(b), List.copyOf(lines)));
    }
    return List.copyOf(result);
  }

  @Override
  public String toString() {
    return "RawPage[pageNumber=" + pageNumber + ", textBoxes=" + textBoxCount() + ", chars="
        + charCount() + ", tables=" + tables.size() + "]";
  }
}
