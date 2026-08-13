package sa.ingenious.pdf;

import java.util.Iterator;

/** A closeable, single-pass cursor over page summaries. */
public final class PageSummaryCursor
    implements Iterator<PageSummary>, Iterable<PageSummary>, AutoCloseable {
  private final CursorState<PageSummary> state;

  PageSummaryCursor(CursorBackend<PageSummary> backend) {
    state = new CursorState<>(backend);
  }

  @Override
  public boolean hasNext() {
    return state.hasNext();
  }

  @Override
  public PageSummary next() {
    return state.next();
  }

  @Override
  public Iterator<PageSummary> iterator() {
    return this;
  }

  @Override
  public void close() {
    state.close();
  }
}
