package sa.ingenious.pdf;

import java.util.Iterator;
import java.util.NoSuchElementException;
import java.util.concurrent.atomic.AtomicBoolean;

final class CursorState<T> implements Iterator<T>, Iterable<T>, AutoCloseable {
  private final CursorBackend<T> backend;
  private final AtomicBoolean closed = new AtomicBoolean();
  private T cached;
  private boolean cachedItem;
  private boolean exhausted;

  CursorState(CursorBackend<T> backend) {
    this.backend = backend;
  }

  @Override
  public synchronized boolean hasNext() {
    if (closed.get() || exhausted) {
      return false;
    }
    if (!cachedItem) {
      cached = backend.next();
      if (cached == null) {
        exhausted = true;
        closeAtEnd();
        return false;
      }
      if (closed.get()) {
        cached = null;
        return false;
      }
      cachedItem = true;
    }
    return true;
  }

  @Override
  public synchronized T next() {
    if (!hasNext()) {
      throw new NoSuchElementException("The cursor has no more items");
    }
    var item = cached;
    cached = null;
    cachedItem = false;
    return item;
  }

  @Override
  public Iterator<T> iterator() {
    return this;
  }

  @Override
  public void close() {
    if (closed.compareAndSet(false, true)) {
      try {
        backend.cancel();
      } finally {
        backend.close();
      }
    }
  }

  private void closeAtEnd() {
    if (closed.compareAndSet(false, true)) {
      backend.close();
    }
  }
}
