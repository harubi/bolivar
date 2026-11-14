package sa.ingenious.pdf;

import sa.ingenious.pdf.internal.Columns;
import sa.ingenious.pdf.internal.Schema.Layout.FloatSlot;
import sa.ingenious.pdf.internal.Schema.Layout.IntSlot;

/**
 * One laid-out page: text boxes contain lines, lines contain glyphs. Under bidi, glyphs are the
 * logical-order spans of their line. See {@link TextPage} for what reads in place and what copies.
 */
public final class LayoutPage extends TextPage {
  private final int pageNumber;
  private final double rotate;
  private final BoundingBox bbox;

  LayoutPage(Columns c) {
    super(new TextLayer(c, Math.toIntExact(c.integer(IntSlot.TextEnd)), false));
    pageNumber = Math.toIntExact(c.integer(IntSlot.PageNumber));
    rotate = c.real(FloatSlot.Rotate);
    bbox = c.box(FloatSlot.X0);
  }

  /** The 1-based page number. */
  public int pageNumber() {
    return pageNumber;
  }

  public double rotate() {
    return rotate;
  }

  public BoundingBox bbox() {
    return bbox;
  }

  @Override
  public String toString() {
    return "LayoutPage[pageNumber="
        + pageNumber
        + ", textBoxes="
        + textBoxCount()
        + ", lines="
        + lineCount()
        + ", glyphs="
        + glyphCount()
        + "]";
  }
}
