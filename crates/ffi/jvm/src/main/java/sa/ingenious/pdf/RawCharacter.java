package sa.ingenious.pdf;

import java.util.List;

/**
 * One source glyph with its graphics state. {@code matrix} has six entries; absent values are
 * null.
 */
public record RawCharacter(
    String text,
    BoundingBox bbox,
    String fontName,
    double size,
    boolean upright,
    double advance,
    List<Double> matrix,
    Integer markedContentId,
    String tag,
    String nonStrokingColorSpace,
    String strokingColorSpace,
    List<Double> nonStrokingColor,
    List<Double> strokingColor) {}
