package sa.ingenious.pdf;

import java.util.List;

public record PageTextBox(
    BoundingBox bbox, String text, String textWithoutBidi, List<PageTextLine> lines) {}
