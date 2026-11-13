package sa.ingenious.pdf;

public record RawTableCell(
    int rowIndex,
    int columnIndex,
    int rowSpan,
    int columnSpan,
    RawTableBoundingBox bbox,
    String text) {}
