package sa.ingenious.pdf;

import java.util.List;
import org.jspecify.annotations.Nullable;

/**
 * One text line of a page, read in place from the page's columns. Views compare by page and
 * position, not by content. It keeps its page alive; call {@link #toRecord()} to keep only the
 * line.
 */
public final class TextLine {
  private final TextLayer layer;
  private final int index;

  TextLine(TextLayer layer, int index) {
    this.layer = layer;
    this.index = index;
  }

  /** The line's position among its page's lines. */
  public int index() {
    return index;
  }

  public BoundingBox bbox() {
    return TextLayer.box(layer.lineBbox, index);
  }

  /** {@code horizontal} or {@code vertical}. */
  public String orientation() {
    return layer.strings[layer.lineOrient[index]];
  }

  /** The line's output text, bidi applied when the document asks for it. */
  public String text() {
    return layer.range(layer.lineText, index);
  }

  /** The line's text in content order, before bidi; null on layout pages, which lack it. */
  public @Nullable String rawText() {
    int[] ranges = layer.lineRawText;
    return ranges == null ? null : layer.range(ranges, index);
  }

  /** The first glyph's index in the page. */
  public int glyphStart() {
    return layer.lineChars[index];
  }

  /** One past the last glyph's index in the page. */
  public int glyphEnd() {
    return layer.lineChars[index + 1];
  }

  public int glyphCount() {
    return glyphEnd() - glyphStart();
  }

  /** The line's glyphs, made on demand. */
  public List<Glyph> glyphs() {
    return TextLayer.list(glyphStart(), glyphEnd(), layer::glyph);
  }

  /** A copy that does not keep the page alive. */
  public LayoutLine toRecord() {
    LayoutChar[] chars = new LayoutChar[glyphCount()];
    for (int g = 0; g < chars.length; g++) {
      chars[g] = layer.glyph(glyphStart() + g).toRecord();
    }
    return new LayoutLine(bbox(), orientation(), text(), TextLayer.wrap(chars));
  }

  /** Views compare by page and position, not by content. */
  @Override
  public boolean equals(@Nullable Object other) {
    return other instanceof TextLine view && view.layer == layer && view.index == index;
  }

  @Override
  public int hashCode() {
    return 31 * System.identityHashCode(layer) + index;
  }

  @Override
  public String toString() {
    return "TextLine[index=" + index + ", glyphs=" + glyphCount() + "]";
  }
}
