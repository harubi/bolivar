package sa.ingenious.pdf;

/**
 * Layout analysis parameters (pdfminer's {@code LAParams}). A null field keeps the native
 * default. {@code boxesFlowDisabled} turns off the boxes-flow ordering, as pdfminer's {@code
 * boxes_flow=None} does; it excludes {@code boxesFlow}.
 */
public record LayoutOptions(
    Double lineOverlap,
    Double charMargin,
    Double lineMargin,
    Double wordMargin,
    Double boxesFlow,
    boolean boxesFlowDisabled,
    Boolean detectVertical,
    Boolean allTexts) {

  public LayoutOptions {
    if (boxesFlowDisabled && boxesFlow != null) {
      throw new IllegalArgumentException("boxesFlow cannot be both set and disabled");
    }
  }

  public static Builder builder() {
    return new Builder();
  }

  public static final class Builder {
    private Double lineOverlap;
    private Double charMargin;
    private Double lineMargin;
    private Double wordMargin;
    private Double boxesFlow;
    private boolean boxesFlowDisabled;
    private Boolean detectVertical;
    private Boolean allTexts;

    private Builder() {}

    public Builder lineOverlap(double value) {
      lineOverlap = value;
      return this;
    }

    public Builder charMargin(double value) {
      charMargin = value;
      return this;
    }

    public Builder lineMargin(double value) {
      lineMargin = value;
      return this;
    }

    public Builder wordMargin(double value) {
      wordMargin = value;
      return this;
    }

    /** Order boxes by this flow, within [-1, 1]. */
    public Builder boxesFlow(double value) {
      boxesFlow = value;
      boxesFlowDisabled = false;
      return this;
    }

    /** Turn off the boxes-flow ordering. */
    public Builder disableBoxesFlow() {
      boxesFlow = null;
      boxesFlowDisabled = true;
      return this;
    }

    public Builder detectVertical(boolean value) {
      detectVertical = value;
      return this;
    }

    public Builder allTexts(boolean value) {
      allTexts = value;
      return this;
    }

    public LayoutOptions build() {
      return new LayoutOptions(
          lineOverlap,
          charMargin,
          lineMargin,
          wordMargin,
          boxesFlow,
          boxesFlowDisabled,
          detectVertical,
          allTexts);
    }
  }
}
