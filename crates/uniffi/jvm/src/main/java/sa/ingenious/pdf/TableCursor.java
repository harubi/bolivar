package sa.ingenious.pdf;

import java.util.Iterator;

/** A closeable, single-pass cursor over extracted tables. */
public final class TableCursor implements Iterator<Table>, Iterable<Table>, AutoCloseable {
  private final CursorState<Table> state;

  TableCursor(CursorBackend<Table> backend) {
    state = new CursorState<>(backend);
  }

  @Override
  public boolean hasNext() {
    return state.hasNext();
  }

  @Override
  public Table next() {
    return state.next();
  }

  @Override
  public Iterator<Table> iterator() {
    return this;
  }

  @Override
  public void close() {
    state.close();
  }
}
