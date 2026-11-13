package sa.ingenious.pdf;

/** One character of a layout line. Under bidi, a logical-order span of the line's text. */
public record LayoutChar(
    String text, BoundingBox bbox, String fontName, double size, boolean upright) {}
