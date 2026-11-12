package sa.ingenious.pdf;

import java.util.Iterator;

/** A closeable, single-pass cursor over the text of each page. */
public final class PageTextCursor
    implements Iterator<PageText>, Iterable<PageText>, AutoCloseable {
  private final CursorState<PageText> state;

  PageTextCursor(CursorBackend<PageText> backend) {
    state = new CursorState<>(backend);
  }

  @Override
  public boolean hasNext() {
    return state.hasNext();
  }

  @Override
  public PageText next() {
    return state.next();
  }

  @Override
  public Iterator<PageText> iterator() {
    return this;
  }

  @Override
  public void close() {
    state.close();
  }
}
