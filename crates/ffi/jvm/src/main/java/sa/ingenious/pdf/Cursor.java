package sa.ingenious.pdf;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.ref.Cleaner;
import java.lang.ref.Reference;
import java.util.ArrayDeque;
import java.util.Iterator;
import java.util.NoSuchElementException;
import java.util.Spliterator;
import java.util.Spliterators;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.Consumer;
import java.util.stream.Stream;
import java.util.stream.StreamSupport;
import sa.ingenious.pdf.internal.Columns;
import sa.ingenious.pdf.internal.Native;

/**
 * A closeable, single-pass cursor that reads one page at a time from native code. Use it in
 * try-with-resources; a cursor that is never closed is freed by a {@link Cleaner}.
 *
 * <p>One thread at a time may iterate; {@link #cancel()} and {@link #close()} may be called from
 * any thread. A native call holds its carrier thread while it runs, so a virtual thread blocked
 * in {@link #hasNext()} pins one carrier.
 */
public final class Cursor<T> implements Iterator<T>, Iterable<T>, AutoCloseable {
  static final Cleaner CLEANER = Cleaner.create();

  /** Turns one batch into zero or more items. */
  @FunctionalInterface
  interface Decoder<T> {
    void decode(Columns columns, Consumer<? super T> out);
  }

  /** Native resources, freed by {@link #close()} or the cleaner; never refers to the cursor. */
  private static final class State implements Runnable {
    // An automatic arena: closing a shared one would cost a JVM-wide handshake per cursor.
    private final MemorySegment scratch = Native.cursorScratch(Arena.ofAuto());
    private final MemorySegment cancel;
    private MemorySegment cursor;
    private final AtomicBoolean released = new AtomicBoolean();

    State(Native.CursorHandles handles) {
      cursor = handles.cursor();
      cancel = handles.cancel();
    }

    /** Free the cursor. Caller holds the cursor's monitor, or the cursor is unreachable. */
    void freeCursor() {
      if (cursor != null) {
        Native.cursorFree(cursor);
        cursor = null;
      }
    }

    @Override
    public void run() {
      if (released.compareAndSet(false, true)) {
        freeCursor();
        Native.cancelFree(cancel);
      }
    }
  }

  private final int kind;
  private final Decoder<T> decoder;
  private final State state;
  private final Cleaner.Cleanable cleanable;
  private final ArrayDeque<T> buffered = new ArrayDeque<>();
  private final AtomicBoolean closing = new AtomicBoolean();
  private final AtomicInteger cancelsInFlight = new AtomicInteger();
  private volatile boolean closed;
  private boolean exhausted;

  Cursor(Native.CursorHandles handles, int kind, Decoder<T> decoder) {
    this.kind = kind;
    this.decoder = decoder;
    this.state = new State(handles);
    this.cleanable = CLEANER.register(this, state);
  }

  @Override
  public boolean hasNext() {
    synchronized (this) {
      try {
        while (buffered.isEmpty()) {
          if (closed || exhausted) {
            return false;
          }
          Columns columns;
          try {
            columns = Native.cursorNext(state.cursor, state.scratch, kind);
          } catch (PdfException.Cancelled cancelled) {
            exhausted = true;
            state.freeCursor();
            if (closed) {
              return false;
            }
            throw cancelled;
          } catch (RuntimeException error) {
            exhausted = true;
            state.freeCursor();
            throw error;
          }
          if (columns == null) {
            exhausted = true;
            state.freeCursor();
            return false;
          }
          try {
            decoder.decode(columns, buffered::add);
          } catch (RuntimeException error) {
            // The batch is gone; going on would skip its page without a word.
            exhausted = true;
            buffered.clear();
            state.freeCursor();
            throw error;
          }
        }
        return true;
      } finally {
        Reference.reachabilityFence(this);
      }
    }
  }

  @Override
  public T next() {
    synchronized (this) {
      if (!hasNext()) {
        throw new NoSuchElementException("The cursor has no more items");
      }
      return buffered.poll();
    }
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
    cancelsInFlight.incrementAndGet();
    try {
      // Read after the increment: close() frees the handle only once this count drains.
      if (!closed) {
        Native.cancel(state.cancel);
      }
    } finally {
      cancelsInFlight.decrementAndGet();
      Reference.reachabilityFence(this);
    }
  }

  /** Cancel any work in progress, wait for it to stop, and free the native cursor. */
  @Override
  public void close() {
    if (!closing.compareAndSet(false, true)) {
      return;
    }
    try {
      // Closed first: a reader woken by the cancel must see a close, not a Cancelled.
      closed = true;
      Native.cancel(state.cancel);
      synchronized (this) {
        state.freeCursor();
        buffered.clear();
      }
      while (cancelsInFlight.get() != 0) {
        Thread.onSpinWait();
      }
      cleanable.clean();
    } finally {
      Reference.reachabilityFence(this);
    }
  }
}
