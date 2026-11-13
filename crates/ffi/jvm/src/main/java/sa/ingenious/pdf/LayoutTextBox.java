package sa.ingenious.pdf;

import java.util.List;

/** One text box: {@code writingMode} is {@code lr-tb} or {@code tb-rl}. */
public record LayoutTextBox(
    BoundingBox bbox, String writingMode, String text, List<LayoutLine> lines) {}
