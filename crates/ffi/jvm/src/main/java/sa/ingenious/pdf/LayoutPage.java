package sa.ingenious.pdf;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import sa.ingenious.pdf.internal.Columns;
import sa.ingenious.pdf.internal.Schema.Layout;

/**
 * One laid-out page, held as columns: text boxes contain lines, lines contain characters, and
 * each level is addressed by index. Under bidi, characters are the logical-order spans of their
 * line.
 *
 * <p>Indexed accessors read the columns directly. {@link #charAt}, {@link #textBoxes} and the
 * column getters copy, so what they return does not keep the page alive.
 */
public final class LayoutPage {
  private final int pageNumber;
  private final double rotate;
  private final BoundingBox bbox;
  private final String text;
  private final String allText;
  private final String[] strings;
  private final double[] boxBbox;
  private final int[] boxMode;
  private final int[] boxText;
  private final int[] boxLines;
  private final double[] lineBbox;
  private final int[] lineOrient;
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

  LayoutPage(Columns c) {
    long[] ints = c.i64(Layout.Ints);
    double[] floats = c.f64(Layout.Floats);
    pageNumber = Math.toIntExact(ints[0]);
    text = c.range((int) ints[1], (int) ints[2]);
    rotate = floats[0];
    bbox = new BoundingBox(floats[1], floats[2], floats[3], floats[4]);
    allText = c.text();
    strings = c.strings();
    boxBbox = c.f64(Layout.BoxBbox);
    boxMode = c.i32(Layout.BoxMode);
    boxText = c.i32(Layout.BoxText);
    boxLines = c.i32(Layout.BoxLines);
    lineBbox = c.f64(Layout.LineBbox);
    lineOrient = c.i32(Layout.LineOrient);
    lineText = c.i32(Layout.LineText);
    lineChars = c.i32(Layout.LineChars);
    x0 = c.f64(Layout.CharX0);
    y0 = c.f64(Layout.CharY0);
    x1 = c.f64(Layout.CharX1);
    y1 = c.f64(Layout.CharY1);
    size = c.f64(Layout.CharSize);
    font = c.i32(Layout.CharFont);
    upright = c.u8(Layout.CharUpright);
    charText = c.i32(Layout.CharText);
  }

  static BoundingBox box(double[] boxes, int index) {
    int o = 4 * index;
    return new BoundingBox(boxes[o], boxes[o + 1], boxes[o + 2], boxes[o + 3]);
  }

  private String range(int[] ranges, int index) {
    int start = ranges[2 * index];
    return start < 0 ? null : allText.substring(start, ranges[2 * index + 1]);
  }

  public int pageNumber() {
    return pageNumber;
  }

  public double rotate() {
    return rotate;
  }

  public BoundingBox bbox() {
    return bbox;
  }

  /** The page's text: its boxes' texts joined by newlines. */
  public String text() {
    return text;
  }

  // ---- text boxes ---------------------------------------------------------------------------

  public int textBoxCount() {
    return boxMode.length;
  }

  public BoundingBox textBoxBbox(int box) {
    return box(boxBbox, box);
  }

  public String textBoxWritingMode(int box) {
    return strings[boxMode[box]];
  }

  public String textBoxText(int box) {
    return range(boxText, box);
  }

  /** The first line of a box. */
  public int textBoxLineStart(int box) {
    return boxLines[box];
  }

  /** One past the last line of a box. */
  public int textBoxLineEnd(int box) {
    return boxLines[box + 1];
  }

  // ---- lines --------------------------------------------------------------------------------

  public int lineCount() {
    return lineOrient.length;
  }

  public BoundingBox lineBbox(int line) {
    return box(lineBbox, line);
  }

  public String lineOrientation(int line) {
    return strings[lineOrient[line]];
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

  public LayoutChar charAt(int i) {
    return new LayoutChar(charText(i), charBbox(i), charFontName(i), charSize(i), charUpright(i));
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

  /** Each character's font name; repeated names are the same string. */
  public String[] charFontNames() {
    String[] names = new String[font.length];
    Arrays.setAll(names, i -> strings[font[i]]);
    return names;
  }

  // ---- records --------------------------------------------------------------------------------

  public List<LayoutTextBox> textBoxes() {
    List<LayoutTextBox> boxes = new ArrayList<>(textBoxCount());
    for (int b = 0; b < textBoxCount(); b++) {
      List<LayoutLine> lines = new ArrayList<>(textBoxLineEnd(b) - textBoxLineStart(b));
      for (int l = textBoxLineStart(b); l < textBoxLineEnd(b); l++) {
        List<LayoutChar> chars = new ArrayList<>(lineCharEnd(l) - lineCharStart(l));
        for (int i = lineCharStart(l); i < lineCharEnd(l); i++) {
          chars.add(charAt(i));
        }
        lines.add(new LayoutLine(lineBbox(l), lineOrientation(l), lineText(l), List.copyOf(chars)));
      }
      boxes.add(
          new LayoutTextBox(
              textBoxBbox(b), textBoxWritingMode(b), textBoxText(b), List.copyOf(lines)));
    }
    return List.copyOf(boxes);
  }

  @Override
  public String toString() {
    return "LayoutPage[pageNumber=" + pageNumber + ", textBoxes=" + textBoxCount() + ", lines="
        + lineCount() + ", chars=" + charCount() + "]";
  }
}
