package sa.ingenious.pdf;

import java.util.List;

/** One text line: {@code orientation} is {@code horizontal} or {@code vertical}. */
public record LayoutLine(
    BoundingBox bbox, String orientation, String text, List<LayoutChar> chars) {}
