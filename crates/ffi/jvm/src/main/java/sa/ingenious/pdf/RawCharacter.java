package sa.ingenious.pdf;

import java.util.List;
import org.jspecify.annotations.Nullable;

/**
 * One source glyph with its graphics state. {@code matrix} has six entries; absent values are null.
 */
public record RawCharacter(
    String text,
    BoundingBox bbox,
    String fontName,
    double size,
    boolean upright,
    double advance,
    List<Double> matrix,
    @Nullable Integer markedContentId,
    @Nullable String tag,
    @Nullable String nonStrokingColorSpace,
    @Nullable String strokingColorSpace,
    @Nullable List<Double> nonStrokingColor,
    @Nullable List<Double> strokingColor) {}
