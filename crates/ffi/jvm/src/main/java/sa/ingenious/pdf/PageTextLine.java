package sa.ingenious.pdf;

/**
 * One text line. {@code text} follows the document's bidi option; {@code textWithoutBidi} is the
 * text a document opened without bidi gives.
 */
public record PageTextLine(BoundingBox bbox, String text, String textWithoutBidi) {}
