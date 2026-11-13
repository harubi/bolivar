package sa.ingenious.pdf;

/** A table box in pdfplumber space: y is measured down from the page top. */
public record RawTableBoundingBox(double x0, double top, double x1, double bottom) {}
