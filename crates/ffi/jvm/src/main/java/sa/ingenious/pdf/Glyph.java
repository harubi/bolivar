package sa.ingenious.pdf;

import org.jspecify.annotations.Nullable;

/**
 * One glyph of a page, read in place from the page's columns. Views compare by page and position,
 * not by content. Under bidi, a layout page's glyphs are the logical-order spans of their line. It
 * keeps its page alive; call {@link #toRecord()} to keep only the glyph.
 */
public final class Glyph {
  private final TextLayer layer;
  private final int index;

  Glyph(TextLayer layer, int index) {
    this.layer = layer;
    this.index = index;
  }

  /** The glyph's position among its page's glyphs; raw pages index their extras by it. */
  public int index() {
    return index;
  }

  public String text() {
    return layer.strings[layer.glyphText[index]];
  }

  public double x0() {
    return layer.x0[index];
  }

  public double y0() {
    return layer.y0[index];
  }

  public double x1() {
    return layer.x1[index];
  }

  public double y1() {
    return layer.y1[index];
  }

  public BoundingBox bbox() {
    return new BoundingBox(x0(), y0(), x1(), y1());
  }

  public String fontName() {
    return layer.strings[layer.font[index]];
  }

  public double size() {
    return layer.size[index];
  }

  public boolean upright() {
    return layer.upright[index] != 0;
  }

  /** A copy that does not keep the page alive. */
  public LayoutChar toRecord() {
    return new LayoutChar(text(), bbox(), fontName(), size(), upright());
  }

  /** Views compare by page and position, not by content. */
  @Override
  public boolean equals(@Nullable Object other) {
    return other instanceof Glyph view && view.layer == layer && view.index == index;
  }

  @Override
  public int hashCode() {
    return 31 * System.identityHashCode(layer) + index;
  }

  @Override
  public String toString() {
    return "Glyph[index=" + index + ", text=" + text() + "]";
  }
}
