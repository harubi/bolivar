package sa.ingenious.pdf;

import java.util.List;
import org.jspecify.annotations.Nullable;

/**
 * Table extraction settings, in pdfplumber's vocabulary. A null field keeps the default. The
 * general tolerances apply to both axes; the axis-specific ones override them. Crops are
 * pdfplumber-space page regions; {@code firstPageCrop} wins over {@code crop} on page one. Values
 * are checked when the cursor opens.
 */
public record TableOptions(
    @Nullable TableStrategy verticalStrategy,
    @Nullable TableStrategy horizontalStrategy,
    @Nullable Double snapTolerance,
    @Nullable Double snapXTolerance,
    @Nullable Double snapYTolerance,
    @Nullable Double joinTolerance,
    @Nullable Double joinXTolerance,
    @Nullable Double joinYTolerance,
    @Nullable Double intersectionTolerance,
    @Nullable Double intersectionXTolerance,
    @Nullable Double intersectionYTolerance,
    @Nullable List<Double> explicitVerticalLines,
    @Nullable List<Double> explicitHorizontalLines,
    @Nullable BoundingBox crop,
    @Nullable BoundingBox firstPageCrop,
    @Nullable Integer maxPages) {

  /** All defaults. */
  public static final TableOptions DEFAULT = builder().build();

  public TableOptions {
    explicitVerticalLines =
        explicitVerticalLines == null ? null : List.copyOf(explicitVerticalLines);
    explicitHorizontalLines =
        explicitHorizontalLines == null ? null : List.copyOf(explicitHorizontalLines);
  }

  public static Builder builder() {
    return new Builder();
  }

  public static final class Builder {
    private @Nullable TableStrategy verticalStrategy;
    private @Nullable TableStrategy horizontalStrategy;
    private @Nullable Double snapTolerance;
    private @Nullable Double snapXTolerance;
    private @Nullable Double snapYTolerance;
    private @Nullable Double joinTolerance;
    private @Nullable Double joinXTolerance;
    private @Nullable Double joinYTolerance;
    private @Nullable Double intersectionTolerance;
    private @Nullable Double intersectionXTolerance;
    private @Nullable Double intersectionYTolerance;
    private @Nullable List<Double> explicitVerticalLines;
    private @Nullable List<Double> explicitHorizontalLines;
    private @Nullable BoundingBox crop;
    private @Nullable BoundingBox firstPageCrop;
    private @Nullable Integer maxPages;

    private Builder() {}

    public Builder verticalStrategy(TableStrategy value) {
      verticalStrategy = value;
      return this;
    }

    public Builder horizontalStrategy(TableStrategy value) {
      horizontalStrategy = value;
      return this;
    }

    public Builder snapTolerance(double value) {
      snapTolerance = value;
      return this;
    }

    public Builder snapXTolerance(double value) {
      snapXTolerance = value;
      return this;
    }

    public Builder snapYTolerance(double value) {
      snapYTolerance = value;
      return this;
    }

    public Builder joinTolerance(double value) {
      joinTolerance = value;
      return this;
    }

    public Builder joinXTolerance(double value) {
      joinXTolerance = value;
      return this;
    }

    public Builder joinYTolerance(double value) {
      joinYTolerance = value;
      return this;
    }

    public Builder intersectionTolerance(double value) {
      intersectionTolerance = value;
      return this;
    }

    public Builder intersectionXTolerance(double value) {
      intersectionXTolerance = value;
      return this;
    }

    public Builder intersectionYTolerance(double value) {
      intersectionYTolerance = value;
      return this;
    }

    public Builder explicitVerticalLines(List<Double> value) {
      explicitVerticalLines = value;
      return this;
    }

    public Builder explicitHorizontalLines(List<Double> value) {
      explicitHorizontalLines = value;
      return this;
    }

    public Builder crop(BoundingBox value) {
      crop = value;
      return this;
    }

    public Builder firstPageCrop(BoundingBox value) {
      firstPageCrop = value;
      return this;
    }

    /**
     * The most pages to read. It narrows the document's selection like {@link
     * CursorOptions#maxPages()}, and when both are set the smaller wins.
     *
     * @deprecated Use {@link CursorOptions.Builder#maxPages(int)}, which every cursor takes.
     */
    @Deprecated
    public Builder maxPages(int value) {
      maxPages = value;
      return this;
    }

    public TableOptions build() {
      return new TableOptions(
          verticalStrategy,
          horizontalStrategy,
          snapTolerance,
          snapXTolerance,
          snapYTolerance,
          joinTolerance,
          joinXTolerance,
          joinYTolerance,
          intersectionTolerance,
          intersectionXTolerance,
          intersectionYTolerance,
          explicitVerticalLines,
          explicitHorizontalLines,
          crop,
          firstPageCrop,
          maxPages);
    }
  }
}
