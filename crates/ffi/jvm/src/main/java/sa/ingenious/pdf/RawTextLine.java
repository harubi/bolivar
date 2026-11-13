package sa.ingenious.pdf;

import java.util.List;

/** One line: {@code rawText} in content order, {@code text} as output (bidi applied if on). */
public record RawTextLine(
    BoundingBox bbox,
    String orientation,
    String rawText,
    String text,
    List<RawCharacter> characters) {}
