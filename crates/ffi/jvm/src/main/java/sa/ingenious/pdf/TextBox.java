package sa.ingenious.pdf;

import java.util.List;
import org.jspecify.annotations.Nullable;

/**
 * One text box of a page, read in place from the page's columns. Views compare by page and
 * position, not by content. It keeps its page alive; call {@link #toRecord()} to keep only the box.
 */
public final class TextBox {
  private final TextLayer layer;
  private final int index;

  TextBox(TextLayer layer, int index) {
    this.layer = layer;
    this.index = index;
  }

  /** The box's position among its page's boxes. */
  public int index() {
    return index;
  }

  public BoundingBox bbox() {
    return TextLayer.box(layer.boxBbox, index);
  }

  /** {@code lr-tb} or {@code tb-rl}. */
  public String writingMode() {
    return layer.strings[layer.boxMode[index]];
  }

  /** The box's text: its lines' texts, joined. */
  public String text() {
    return layer.range(layer.boxText, index);
  }

  /** The first line's index in the page. */
  public int lineStart() {
    return layer.boxLines[index];
  }

  /** One past the last line's index in the page. */
  public int lineEnd() {
    return layer.boxLines[index + 1];
  }

  public int lineCount() {
    return lineEnd() - lineStart();
  }

  /** The box's lines, made on demand. */
  public List<TextLine> lines() {
    return TextLayer.list(lineStart(), lineEnd(), layer::line);
  }

  /** A copy that does not keep the page alive. */
  public LayoutTextBox toRecord() {
    LayoutLine[] lines = new LayoutLine[lineCount()];
    for (int l = 0; l < lines.length; l++) {
      lines[l] = layer.line(lineStart() + l).toRecord();
    }
    return new LayoutTextBox(bbox(), writingMode(), text(), TextLayer.wrap(lines));
  }

  /** Views compare by page and position, not by content. */
  @Override
  public boolean equals(@Nullable Object other) {
    return other instanceof TextBox view && view.layer == layer && view.index == index;
  }

  @Override
  public int hashCode() {
    return 31 * System.identityHashCode(layer) + index;
  }

  @Override
  public String toString() {
    return "TextBox[index=" + index + ", lines=" + lineCount() + "]";
  }
}
