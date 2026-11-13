package sa.ingenious.pdf;

import java.util.List;

/**
 * Table extraction settings, in pdfplumber's vocabulary. A null field keeps the default. The
 * general tolerances apply to both axes; the axis-specific ones override them. Crops are
 * pdfplumber-space page regions; {@code firstPageCrop} wins over {@code crop} on page one.
 * Values are checked when the cursor opens.
 */
public record TableOptions(
    TableStrategy verticalStrategy,
    TableStrategy horizontalStrategy,
    Double snapTolerance,
    Double snapXTolerance,
    Double snapYTolerance,
    Double joinTolerance,
    Double joinXTolerance,
    Double joinYTolerance,
    Double intersectionTolerance,
    Double intersectionXTolerance,
    Double intersectionYTolerance,
    List<Double> explicitVerticalLines,
    List<Double> explicitHorizontalLines,
    BoundingBox crop,
    BoundingBox firstPageCrop,
    Integer maxPages) {

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
    private TableStrategy verticalStrategy;
    private TableStrategy horizontalStrategy;
    private Double snapTolerance;
    private Double snapXTolerance;
    private Double snapYTolerance;
    private Double joinTolerance;
    private Double joinXTolerance;
    private Double joinYTolerance;
    private Double intersectionTolerance;
    private Double intersectionXTolerance;
    private Double intersectionYTolerance;
    private List<Double> explicitVerticalLines;
    private List<Double> explicitHorizontalLines;
    private BoundingBox crop;
    private BoundingBox firstPageCrop;
    private Integer maxPages;

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
