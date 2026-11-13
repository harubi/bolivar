package sa.ingenious.pdf;

import java.util.List;

/**
 * Document options. A null field keeps the native default: no password, every page, no page
 * limit, caching on, bidi off, default layout. Values are checked when the document opens.
 *
 * @param pages 1-based page numbers to read; pages always come in document order, each once
 */
public record Options(
    String password,
    List<Integer> pages,
    Integer maxPages,
    Boolean caching,
    Boolean bidi,
    LayoutOptions layout) {

  /** All defaults. */
  public static final Options DEFAULT = new Options(null, null, null, null, null, null);

  public Options {
    pages = pages == null ? null : List.copyOf(pages);
  }

  public static Builder builder() {
    return new Builder();
  }

  public static final class Builder {
    private String password;
    private List<Integer> pages;
    private Integer maxPages;
    private Boolean caching;
    private Boolean bidi;
    private LayoutOptions layout;

    private Builder() {}

    public Builder password(String value) {
      password = value;
      return this;
    }

    public Builder pages(int... values) {
      pages = java.util.Arrays.stream(values).boxed().toList();
      return this;
    }

    public Builder pages(List<Integer> values) {
      pages = values;
      return this;
    }

    public Builder maxPages(int value) {
      maxPages = value;
      return this;
    }

    public Builder caching(boolean value) {
      caching = value;
      return this;
    }

    /** Reorder right-to-left text into logical order with ICU. */
    public Builder bidi(boolean value) {
      bidi = value;
      return this;
    }

    public Builder layout(LayoutOptions value) {
      layout = value;
      return this;
    }

    public Options build() {
      return new Options(password, pages, maxPages, caching, bidi, layout);
    }
  }
}
