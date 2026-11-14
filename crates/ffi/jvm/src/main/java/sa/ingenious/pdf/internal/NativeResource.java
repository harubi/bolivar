package sa.ingenious.pdf.internal;

import java.lang.foreign.MemorySegment;
import java.lang.ref.Cleaner;
import java.util.concurrent.locks.StampedLock;
import java.util.function.Consumer;

/**
 * A native handle with a lease: calls hold a shared lease while they run, and {@link #close} takes
 * the exclusive one, so a handle is never freed under a call. A handle whose owner becomes
 * unreachable unclosed is freed by a {@link Cleaner}. Not part of the public API.
 *
 * <p>Owners use it by composition and fence themselves after each call ({@link
 * java.lang.ref.Reference#reachabilityFence}), so the cleaner cannot run while a call that started
 * from the owner is still in native code.
 */
public final class NativeResource {
  private static final Cleaner CLEANER = Cleaner.create();

  /** Frees the handle. Holds only the handle and a static free function, never the owner. */
  private record Free(MemorySegment handle, Consumer<MemorySegment> free) implements Runnable {
    @Override
    public void run() {
      free.accept(handle);
    }
  }

  private final MemorySegment handle;
  private final Cleaner.Cleanable cleanable;
  private final StampedLock lock = new StampedLock();
  private boolean closed; // Written under the write lock, read under either.

  /**
   * Own {@code handle} on behalf of {@code owner}. {@code free} must not capture the owner: pass a
   * static method reference.
   */
  public NativeResource(Object owner, MemorySegment handle, Consumer<MemorySegment> free) {
    this.handle = handle;
    this.cleanable = CLEANER.register(owner, new Free(handle, free));
  }

  /** The handle. Use it only between {@link #acquire} and {@link #release}. */
  public MemorySegment handle() {
    return handle;
  }

  /**
   * Take a shared lease, waiting while a close is in progress.
   *
   * @return the lease stamp, or 0 when the handle is closed
   */
  public long acquire() {
    long stamp = lock.readLock();
    if (closed) {
      lock.unlockRead(stamp);
      return 0;
    }
    return stamp;
  }

  /** Return a lease from {@link #acquire}. */
  public void release(long stamp) {
    lock.unlockRead(stamp);
  }

  /**
   * Wait for calls in progress, then free the handle. Idempotent.
   *
   * @return whether this call freed it
   */
  public boolean close() {
    long stamp = lock.writeLock();
    try {
      if (closed) {
        return false;
      }
      closed = true;
      cleanable.clean();
      return true;
    } finally {
      lock.unlockWrite(stamp);
    }
  }
}
