package sa.ingenious.pdf;

import java.util.List;

/**
 * The text of one page with no per-character data: its layout box, text boxes, the sorted fonts of
 * every character whose text is not blank, and the sorted font and whole size of every such
 * character holding a decimal digit.
 */
public record PageText(
    int pageNumber,
    BoundingBox bbox,
    List<PageTextBox> textBoxes,
    List<String> fonts,
    List<DigitStyle> digitStyles) {}
