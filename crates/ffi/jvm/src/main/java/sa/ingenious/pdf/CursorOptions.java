package sa.ingenious.pdf;

import java.util.List;
import org.jspecify.annotations.Nullable;

/**
 * Which pages one cursor reads and how far ahead it runs. A null field keeps the default: every
 * page, nothing skipped, no limit, a read-ahead of 50. Values are checked when the cursor opens.
 *
 * <p>A cursor only narrows its document's selection: it starts from the pages the document's {@link
 * Options} select (the pages {@link Document#pageCount()} counts), keeps {@code pages}, passes over
 * the first {@code skip} of those, then reads at most {@code maxPages}. Pages come in document
 * order, each once, and page numbers the file does not have are ignored. So a cursor with {@code
 * maxPages(2)} followed by one with {@code skip(2)} reads every selected page exactly once.
 *
 * @param pages 1-based page numbers to keep; an empty list keeps none
 * @param skip how many of the kept pages to pass over first; not negative
 * @param maxPages the most pages to read; positive
 * @param readAhead how many pages may be started or finished but not yet read, ahead of the reader;
 *     positive. It also caps the cursor's parallelism at {@code min(workers, readAhead)} and the
 *     finished pages that wait behind a slow one at {@code readAhead - 1}
 */
public record CursorOptions(
    @Nullable List<Integer> pages,
    @Nullable Integer skip,
    @Nullable Integer maxPages,
    @Nullable Integer readAhead) {

  /** All defaults. */
  public static final CursorOptions DEFAULT = new CursorOptions(null, null, null, null);

  public CursorOptions {
    pages = pages == null ? null : List.copyOf(pages);
  }

  public static Builder builder() {
    return new Builder();
  }

  public static final class Builder {
    private @Nullable List<Integer> pages;
    private @Nullable Integer skip;
    private @Nullable Integer maxPages;
    private @Nullable Integer readAhead;

    private Builder() {}

    /** Keep only these 1-based pages; missing ones are ignored and repeats count once. */
    public Builder pages(int... values) {
      pages = java.util.Arrays.stream(values).boxed().toList();
      return this;
    }

    /** As {@link #pages(int...)}; an empty list keeps no page. */
    public Builder pages(List<Integer> values) {
      pages = values;
      return this;
    }

    /** Pass over this many of the kept pages first; must not be negative. */
    public Builder skip(int value) {
      skip = value;
      return this;
    }

    /** Read at most this many pages after the skip; must be positive. */
    public Builder maxPages(int value) {
      maxPages = value;
      return this;
    }

    /** Let at most this many pages run ahead of the reader; must be positive. */
    public Builder readAhead(int value) {
      readAhead = value;
      return this;
    }

    public CursorOptions build() {
      return new CursorOptions(pages, skip, maxPages, readAhead);
    }
  }
}
