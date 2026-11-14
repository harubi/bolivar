package sa.ingenious.pdf;

import java.util.List;

/**
 * What layout and raw pages share: text boxes made of lines made of glyphs, held as columns.
 *
 * <p>The views ({@link TextBox}, {@link TextLine}, {@link Glyph}) read the columns in place and
 * keep the page alive. {@code toRecord} on a view, {@link #toRecords()} and the {@code glyph...s}
 * column getters copy, so what they return does not.
 */
public abstract sealed class TextPage permits LayoutPage, RawPage {
  final TextLayer layer;

  TextPage(TextLayer layer) {
    this.layer = layer;
  }

  /** The page's text: its boxes' texts joined by newlines. */
  public final String text() {
    return layer.pageText();
  }

  public final int textBoxCount() {
    return layer.boxCount();
  }

  public final TextBox textBox(int index) {
    return layer.box(index);
  }

  /** The page's text boxes, made on demand. */
  public final List<TextBox> textBoxes() {
    return TextLayer.list(0, layer.boxCount(), layer::box);
  }

  public final int lineCount() {
    return layer.lineCount();
  }

  public final TextLine line(int index) {
    return layer.line(index);
  }

  /** Every line of the page, made on demand. */
  public final List<TextLine> lines() {
    return TextLayer.list(0, layer.lineCount(), layer::line);
  }

  public final int glyphCount() {
    return layer.glyphCount();
  }

  public final Glyph glyph(int index) {
    return layer.glyph(index);
  }

  /** Every glyph of the page, made on demand. */
  public final List<Glyph> glyphs() {
    return TextLayer.list(0, layer.glyphCount(), layer::glyph);
  }

  /** The text boxes, copied into records that do not keep the page alive. */
  public final List<LayoutTextBox> toRecords() {
    LayoutTextBox[] boxes = new LayoutTextBox[layer.boxCount()];
    for (int b = 0; b < boxes.length; b++) {
      boxes[b] = layer.box(b).toRecord();
    }
    return TextLayer.wrap(boxes);
  }

  // ---- whole columns (copies) -------------------------------------------------------------

  public final double[] glyphX0s() {
    return layer.x0.clone();
  }

  public final double[] glyphY0s() {
    return layer.y0.clone();
  }

  public final double[] glyphX1s() {
    return layer.x1.clone();
  }

  public final double[] glyphY1s() {
    return layer.y1.clone();
  }

  public final double[] glyphSizes() {
    return layer.size.clone();
  }

  public final boolean[] glyphUprights() {
    boolean[] values = new boolean[layer.upright.length];
    for (int i = 0; i < values.length; i++) {
      values[i] = layer.upright[i] != 0;
    }
    return values;
  }

  /** Each glyph's font name; repeated names are the same string. */
  public final String[] glyphFontNames() {
    return strings(layer.font);
  }

  /** Each glyph's text; repeated texts are the same string. */
  public final String[] glyphTexts() {
    return strings(layer.glyphText);
  }

  private String[] strings(int[] ids) {
    String[] values = new String[ids.length];
    for (int i = 0; i < values.length; i++) {
      values[i] = layer.strings[ids[i]];
    }
    return values;
  }
}
