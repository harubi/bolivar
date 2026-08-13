package sa.ingenious.pdf;

import java.util.Iterator;

/** A closeable, single-pass cursor over per-page table rows. */
public final class PageTableRowsCursor
    implements Iterator<PageTableRows>, Iterable<PageTableRows>, AutoCloseable {
  private final CursorState<PageTableRows> state;

  PageTableRowsCursor(CursorBackend<PageTableRows> backend) {
    state = new CursorState<>(backend);
  }

  @Override
  public boolean hasNext() {
    return state.hasNext();
  }

  @Override
  public PageTableRows next() {
    return state.next();
  }

  @Override
  public Iterator<PageTableRows> iterator() {
    return this;
  }

  @Override
  public void close() {
    state.close();
  }
}
