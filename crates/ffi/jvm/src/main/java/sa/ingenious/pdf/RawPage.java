package sa.ingenious.pdf;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Objects;
import org.jspecify.annotations.Nullable;
import sa.ingenious.pdf.internal.Columns;
import sa.ingenious.pdf.internal.Schema.Raw.Col;
import sa.ingenious.pdf.internal.Schema.Raw.FloatSlot;
import sa.ingenious.pdf.internal.Schema.Raw.IntSlot;

/**
 * One page with nothing dropped: PDF identity and boxes, text boxes and lines with raw and output
 * text, every source glyph with its graphics state, and the page's tables in pdfplumber space.
 *
 * <p>Glyphs are the views of {@link TextPage}; their graphics state is read here by glyph index
 * ({@link Glyph#index()}). {@link #character}, {@link #rawTextBoxes} and {@link #tables} copy, so
 * what they return does not keep the page alive.
 */
public final class RawPage extends TextPage {
  private final int pageIndex;
  private final long objectId;
  private final @Nullable String label;
  private final long rotation;
  private final double userUnit;
  private final BoundingBox layoutBbox;
  private final double[] pageBoxes;
  private final byte[] pageBoxPresent;
  private final double[] advance;
  private final double[] matrix;
  private final int[] mcid;
  private final int[] tag;
  private final int[] ncs;
  private final int[] scs;
  private final int[] ncolor;
  private final int[] scolor;
  private final double[] colorPool;
  private final double[] tableBbox;
  private final int[] tableShape;
  private final int[] tableCells;
  private final int[] cellIndex;
  private final double[] cellBbox;
  private final int[] cellText;

  RawPage(Columns c) {
    super(new TextLayer(c, Math.toIntExact(c.integer(IntSlot.TextEnd)), true));
    pageIndex = Math.toIntExact(c.integer(IntSlot.PageIndex));
    objectId = c.integer(IntSlot.ObjectId);
    rotation = c.integer(IntSlot.Rotation);
    label =
        c.range(
            Math.toIntExact(c.integer(IntSlot.LabelStart)),
            Math.toIntExact(c.integer(IntSlot.LabelEnd)));
    userUnit = c.real(FloatSlot.UserUnit);
    layoutBbox =
        new BoundingBox(
            c.real(FloatSlot.X0), c.real(FloatSlot.Y0), c.real(FloatSlot.X1), c.real(FloatSlot.Y1));
    pageBoxes = c.f64(Col.PageBoxes);
    pageBoxPresent = c.u8(Col.PageBoxPresent);
    advance = c.f64(Col.CharAdvance);
    matrix = c.f64(Col.CharMatrix);
    mcid = c.i32(Col.CharMcid);
    tag = c.i32(Col.CharTag);
    ncs = c.i32(Col.CharNcs);
    scs = c.i32(Col.CharScs);
    ncolor = c.i32(Col.CharNcolor);
    scolor = c.i32(Col.CharScolor);
    colorPool = c.f64(Col.ColorPool);
    tableBbox = c.f64(Col.TableBboxTopDown);
    tableShape = c.i32(Col.TableShape);
    tableCells = c.i32(Col.TableCells);
    cellIndex = c.i32(Col.CellIndex);
    cellBbox = c.f64(Col.CellBboxTopDown);
    cellText = c.i32(Col.CellText);
  }

  /** The 0-based index of the page in the document. */
  public int pageIndex() {
    return pageIndex;
  }

  /** The 1-based page number: {@link #pageIndex()} plus one. */
  public int pageNumber() {
    return pageIndex + 1;
  }

  /** The page object's number in the PDF. */
  public long objectId() {
    return objectId;
  }

  /** The page label (e.g. {@code iv}), or null. */
  public @Nullable String label() {
    return label;
  }

  public long rotation() {
    return rotation;
  }

  public double userUnit() {
    return userUnit;
  }

  public BoundingBox layoutBbox() {
    return layoutBbox;
  }

  /** The media, crop, bleed, trim and art boxes, as written. */
  public RawPageBoxes boxes() {
    List<@Nullable List<Double>> boxes = new ArrayList<>(5);
    int offset = 0;
    for (byte present : pageBoxPresent) {
      if (present != 0) {
        boxes.add(
            List.of(
                pageBoxes[offset],
                pageBoxes[offset + 1],
                pageBoxes[offset + 2],
                pageBoxes[offset + 3]));
        offset += 4;
      } else {
        boxes.add(null);
      }
    }
    return new RawPageBoxes(boxes.get(0), boxes.get(1), boxes.get(2), boxes.get(3), boxes.get(4));
  }

  // ---- glyph graphics state, by glyph index -----------------------------------------------

  public double glyphAdvance(int glyph) {
    return advance[glyph];
  }

  /** Entry {@code k} (0 to 5) of the glyph's text matrix. */
  public double glyphMatrix(int glyph, int k) {
    Objects.checkIndex(glyph, advance.length);
    return matrix[6 * glyph + Objects.checkIndex(k, 6)];
  }

  /** The marked content id, or null. */
  public @Nullable Integer glyphMarkedContentId(int glyph) {
    return mcid[glyph] == Integer.MIN_VALUE ? null : mcid[glyph];
  }

  public @Nullable String glyphTag(int glyph) {
    return string(tag[glyph]);
  }

  public @Nullable String glyphNonStrokingColorSpace(int glyph) {
    return string(ncs[glyph]);
  }

  public @Nullable String glyphStrokingColorSpace(int glyph) {
    return string(scs[glyph]);
  }

  /** The non-stroking colour's components, copied, or null. */
  public double @Nullable [] glyphNonStrokingColor(int glyph) {
    return color(ncolor, glyph);
  }

  /** The stroking colour's components, copied, or null. */
  public double @Nullable [] glyphStrokingColor(int glyph) {
    return color(scolor, glyph);
  }

  private @Nullable String string(int id) {
    return id < 0 ? null : layer.strings[id];
  }

  private double @Nullable [] color(int[] ranges, int glyph) {
    int start = ranges[2 * glyph];
    return start < 0 ? null : Arrays.copyOfRange(colorPool, start, ranges[2 * glyph + 1]);
  }

  private @Nullable List<Double> colorList(int[] ranges, int glyph) {
    int start = ranges[2 * glyph];
    if (start < 0) {
      return null;
    }
    Double[] values = new Double[ranges[2 * glyph + 1] - start];
    for (int k = 0; k < values.length; k++) {
      values[k] = colorPool[start + k];
    }
    return TextLayer.wrap(values);
  }

  /** Every glyph's advance, copied. */
  public double[] glyphAdvances() {
    return advance.clone();
  }

  /** Every glyph's text matrix, six entries per glyph, copied. */
  public double[] glyphMatrices() {
    return matrix.clone();
  }

  /** One glyph with its graphics state, as a record. */
  public RawCharacter character(int glyph) {
    Glyph g = glyph(glyph);
    int m = 6 * glyph;
    return new RawCharacter(
        g.text(),
        g.bbox(),
        g.fontName(),
        g.size(),
        g.upright(),
        advance[glyph],
        List.of(
            matrix[m], matrix[m + 1], matrix[m + 2], matrix[m + 3], matrix[m + 4], matrix[m + 5]),
        glyphMarkedContentId(glyph),
        glyphTag(glyph),
        glyphNonStrokingColorSpace(glyph),
        glyphStrokingColorSpace(glyph),
        colorList(ncolor, glyph),
        colorList(scolor, glyph));
  }

  /** The text boxes with raw text and graphics state, as records. */
  public List<RawTextBox> rawTextBoxes() {
    RawTextBox[] boxes = new RawTextBox[textBoxCount()];
    for (int b = 0; b < boxes.length; b++) {
      TextBox box = textBox(b);
      RawTextLine[] lines = new RawTextLine[box.lineCount()];
      for (int l = 0; l < lines.length; l++) {
        TextLine line = line(box.lineStart() + l);
        RawCharacter[] characters = new RawCharacter[line.glyphCount()];
        for (int g = 0; g < characters.length; g++) {
          characters[g] = character(line.glyphStart() + g);
        }
        String text = line.text();
        String rawText = line.rawText();
        lines[l] =
            new RawTextLine(
                line.bbox(),
                line.orientation(),
                rawText == null ? text : rawText,
                text,
                TextLayer.wrap(characters));
      }
      boxes[b] = new RawTextBox(box.bbox(), box.writingMode(), box.text(), TextLayer.wrap(lines));
    }
    return TextLayer.wrap(boxes);
  }

  // ---- tables -----------------------------------------------------------------------------

  private RawTableBoundingBox tableBox(double[] boxes, int index) {
    int o = 4 * index;
    return new RawTableBoundingBox(boxes[o], boxes[o + 1], boxes[o + 2], boxes[o + 3]);
  }

  public int tableCount() {
    return tableShape.length / 2;
  }

  /** The page's tables in pdfplumber space, as records. */
  public List<RawTable> tables() {
    RawTable[] tables = new RawTable[tableCount()];
    for (int t = 0; t < tables.length; t++) {
      RawTableCell[] cells = new RawTableCell[tableCells[t + 1] - tableCells[t]];
      for (int i = tableCells[t]; i < tableCells[t + 1]; i++) {
        cells[i - tableCells[t]] =
            new RawTableCell(
                cellIndex[4 * i],
                cellIndex[4 * i + 1],
                cellIndex[4 * i + 2],
                cellIndex[4 * i + 3],
                tableBox(cellBbox, i),
                layer.range(cellText, i));
      }
      tables[t] =
          new RawTable(
              tableBox(tableBbox, t),
              tableShape[2 * t],
              tableShape[2 * t + 1],
              TextLayer.wrap(cells));
    }
    return TextLayer.wrap(tables);
  }

  @Override
  public String toString() {
    return "RawPage[pageNumber="
        + pageNumber()
        + ", textBoxes="
        + textBoxCount()
        + ", glyphs="
        + glyphCount()
        + ", tables="
        + tableCount()
        + "]";
  }
}
