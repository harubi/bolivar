package sa.ingenious.pdf;

/** A table cell; its box is in PDF space. */
public record TableCell(
    int rowIndex, int columnIndex, int rowSpan, int columnSpan, BoundingBox bbox, String text) {}
