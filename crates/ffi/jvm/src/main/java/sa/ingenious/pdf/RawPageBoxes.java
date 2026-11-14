package sa.ingenious.pdf;

import java.util.List;
import org.jspecify.annotations.Nullable;

/** The page's PDF boxes, each {@code [x0 y0 x1 y1]} as written, or null when absent. */
public record RawPageBoxes(
    @Nullable List<Double> media,
    @Nullable List<Double> crop,
    @Nullable List<Double> bleed,
    @Nullable List<Double> trim,
    @Nullable List<Double> art) {}
