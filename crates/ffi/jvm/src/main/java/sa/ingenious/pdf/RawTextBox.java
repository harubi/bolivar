package sa.ingenious.pdf;

import java.util.List;

public record RawTextBox(
    BoundingBox bbox, String writingMode, String text, List<RawTextLine> lines) {}
