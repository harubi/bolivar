package sa.ingenious.pdf;

import java.util.AbstractList;
import java.util.Objects;
import java.util.RandomAccess;
import java.util.function.IntFunction;

/** An unmodifiable list whose items are made on demand from an index range. */
final class IndexedList<T> extends AbstractList<T> implements RandomAccess {
  private final int from;
  private final int size;
  private final IntFunction<T> item;

  IndexedList(int from, int to, IntFunction<T> item) {
    this.from = from;
    this.size = to - from;
    this.item = item;
  }

  @Override
  public T get(int index) {
    Objects.checkIndex(index, size);
    return item.apply(from + index);
  }

  @Override
  public int size() {
    return size;
  }
}
