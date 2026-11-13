package sa.ingenious.pdf;

import java.util.List;

/** A table in pdfplumber space, as the raw page holds it. */
public record RawTable(
    RawTableBoundingBox bbox, int rowCount, int columnCount, List<RawTableCell> cells) {}
