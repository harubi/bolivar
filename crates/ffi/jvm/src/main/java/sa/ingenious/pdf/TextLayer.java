package sa.ingenious.pdf;

import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Objects;
import org.jspecify.annotations.Nullable;
import sa.ingenious.pdf.internal.Columns;
import sa.ingenious.pdf.internal.Schema.Layout.Col;
import sa.ingenious.pdf.internal.Schema.Raw;

/**
 * The text boxes, lines and glyphs of a layout or raw page, as columns. Raw batches start with the
 * layout columns, so one reader serves both. The views ({@link TextBox}, {@link TextLine}, {@link
 * Glyph}) read it in place.
 */
final class TextLayer {
  final char[] text;
  final String[] strings;
  final int textEnd;
  final double[] boxBbox;
  final int[] boxMode;
  final int[] boxText;
  final int[] boxLines;
  final double[] lineBbox;
  final int[] lineOrient;
  final int[] lineText;
  final int[] lineChars;
  final int @Nullable [] lineRawText;
  final double[] x0;
  final double[] y0;
  final double[] x1;
  final double[] y1;
  final double[] size;
  final int[] font;
  final byte[] upright;
  final int[] glyphText;

  TextLayer(Columns c, int textEnd, boolean raw) {
    this.text = c.text();
    this.strings = c.strings();
    this.textEnd = textEnd;
    boxBbox = c.f64(Col.BoxBbox);
    boxMode = c.i32(Col.BoxMode);
    boxText = c.i32(Col.BoxText);
    boxLines = c.i32(Col.BoxLines);
    lineBbox = c.f64(Col.LineBbox);
    lineOrient = c.i32(Col.LineOrient);
    lineText = c.i32(Col.LineText);
    lineChars = c.i32(Col.LineChars);
    lineRawText = raw ? c.i32(Raw.Col.LineRawText) : null;
    x0 = c.f64(Col.CharX0);
    y0 = c.f64(Col.CharY0);
    x1 = c.f64(Col.CharX1);
    y1 = c.f64(Col.CharY1);
    size = c.f64(Col.CharSize);
    font = c.i32(Col.CharFont);
    upright = c.u8(Col.CharUpright);
    glyphText = c.i32(Col.CharText);
  }

  static BoundingBox box(double[] boxes, int index) {
    int o = 4 * index;
    return new BoundingBox(boxes[o], boxes[o + 1], boxes[o + 2], boxes[o + 3]);
  }

  /** The text of the {@code index}th range; ranges in the text layer are never absent. */
  String range(int[] ranges, int index) {
    int start = ranges[2 * index];
    return new String(text, start, ranges[2 * index + 1] - start);
  }

  /** An interned string, or null for a negative id. */
  @Nullable String string(int id) {
    return id < 0 ? null : strings[id];
  }

  /** Whether the {@code index}th ranges of two range columns are the same span. */
  static boolean sameRange(int[] a, int[] b, int index) {
    return a[2 * index] == b[2 * index] && a[2 * index + 1] == b[2 * index + 1];
  }

  String pageText() {
    return new String(text, 0, textEnd);
  }

  int boxCount() {
    return boxMode.length;
  }

  int lineCount() {
    return lineOrient.length;
  }

  int glyphCount() {
    return font.length;
  }

  TextBox box(int index) {
    return new TextBox(this, Objects.checkIndex(index, boxCount()));
  }

  TextLine line(int index) {
    return new TextLine(this, Objects.checkIndex(index, lineCount()));
  }

  Glyph glyph(int index) {
    return new Glyph(this, Objects.checkIndex(index, glyphCount()));
  }

  /** An unmodifiable list over {@code items}, without copying them. */
  static <T extends @Nullable Object> List<T> wrap(T[] items) {
    return Collections.unmodifiableList(Arrays.asList(items));
  }
}
