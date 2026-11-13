package sa.ingenious.pdf;

import java.util.List;

/** The page's PDF boxes, each {@code [x0 y0 x1 y1]} as written, or null when absent. */
public record RawPageBoxes(
    List<Double> media, List<Double> crop, List<Double> bleed, List<Double> trim, List<Double> art) {}
