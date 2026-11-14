package sa.ingenious.pdf;

import java.lang.foreign.MemorySegment;
import java.lang.ref.Reference;
import java.util.ArrayDeque;
import java.util.Iterator;
import java.util.NoSuchElementException;
import java.util.Spliterator;
import java.util.Spliterators;
import java.util.function.Consumer;
import java.util.stream.Stream;
import java.util.stream.StreamSupport;
import org.jspecify.annotations.Nullable;
import sa.ingenious.pdf.internal.Columns;
import sa.ingenious.pdf.internal.Native;
import sa.ingenious.pdf.internal.NativeResource;
import sa.ingenious.pdf.internal.Schema;

/**
 * A closeable, single-pass cursor that reads one page at a time from native code. Use it in
 * try-with-resources; a cursor that is never closed is freed by a {@link java.lang.ref.Cleaner}.
 *
 * <p>Iteration is serialized: {@link #hasNext()} and {@link #next()} share one monitor, so threads
 * that share a cursor take turns, each item going to exactly one of them. {@link #cancel()} and
 * {@link #close()} stay outside that monitor and may be called from any thread, at any time. A
 * native call holds its carrier thread while it runs, so a virtual thread blocked in {@link
 * #hasNext()} pins one carrier.
 *
 * <p>It is both the iterator and its own {@link Iterable}, so it works in a for-each loop; being
 * single-pass, {@link #iterator()} returns this cursor, not a fresh pass.
 *
 * @param <T> the item type
 */
@SuppressWarnings("IterableAndIterator") // Single-pass by design; see above.
public final class Cursor<T> implements Iterator<T>, Iterable<T>, AutoCloseable {
  /** Turns one batch into zero or more items. */
  @FunctionalInterface
  interface Decoder<T> {
    void decode(Columns columns, Consumer<? super T> out);
  }

  private final NativeResource resource;
  private final Schema.Kind kind;
  private final Decoder<T> decoder;
  private final Native.Scratch scratch = Native.Scratch.allocate();
  private final ArrayDeque<T> buffered = new ArrayDeque<>();
  private volatile boolean closing;
  private boolean exhausted;

  Cursor(MemorySegment handle, Schema.Kind kind, Decoder<T> decoder) {
    this.resource = new NativeResource(this, handle, Native::cursorFree);
    this.kind = kind;
    this.decoder = decoder;
  }

  private @Nullable Columns nextBatch() {
    long stamp = resource.acquire();
    if (stamp == 0) {
      return null;
    }
    try {
      return Native.cursorNext(resource.handle(), scratch, kind);
    } catch (PdfException.Cancelled cancelled) {
      if (closing) {
        return null;
      }
      throw cancelled;
    } finally {
      resource.release(stamp);
      Reference.reachabilityFence(this);
    }
  }

  // Synchronized: native calls share one scratch out-slot and ErrorOut, and `buffered` is a
  // plain deque. Two threads iterating at once would otherwise read the same batch and free it
  // twice.
  @Override
  public synchronized boolean hasNext() {
    // Items already read stay deliverable after a close, so a true hasNext() always lets next()
    // succeed; only reading more stops.
    while (buffered.isEmpty()) {
      if (exhausted || closing) {
        return false;
      }
      Columns columns;
      try {
        columns = nextBatch();
      } catch (RuntimeException error) {
        exhausted = true;
        throw error;
      }
      if (columns == null) {
        exhausted = true;
        return false;
      }
      try {
        decoder.decode(columns, buffered::add);
      } catch (RuntimeException error) {
        // The batch is gone; going on would skip its page without a word.
        exhausted = true;
        buffered.clear();
        throw error;
      }
    }
    return true;
  }

  @Override
  public synchronized T next() {
    if (!hasNext()) {
      throw new NoSuchElementException("The cursor has no more items");
    }
    return buffered.removeFirst();
  }

  @Override
  public Iterator<T> iterator() {
    return this;
  }

  /** The remaining items as a sequential stream; closing the stream closes the cursor. */
  public Stream<T> stream() {
    return StreamSupport.stream(
            Spliterators.spliteratorUnknownSize(this, Spliterator.ORDERED | Spliterator.NONNULL),
            false)
        .onClose(this::close);
  }

  /**
   * Ask the native side to stop. A blocked {@link #hasNext()} then throws {@link
   * PdfException.Cancelled}. Safe from any thread, and after close.
   */
  public void cancel() {
    long stamp = resource.acquire();
    if (stamp == 0) {
      return;
    }
    try {
      Native.cursorCancel(resource.handle());
    } finally {
      resource.release(stamp);
      Reference.reachabilityFence(this);
    }
  }

  /**
   * Stop any work in progress, wait for it, and free the native cursor. Iteration then ends
   * quietly. Idempotent.
   */
  @Override
  public void close() {
    // Closing first, so a reader woken by the cancel sees a close, not a Cancelled; the cancel
    // comes before the exclusive lease, so close never waits out a whole page.
    closing = true;
    try {
      cancel();
      resource.close();
    } finally {
      Reference.reachabilityFence(this);
    }
  }
}
