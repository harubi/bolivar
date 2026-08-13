package sa.ingenious.pdf;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.concurrent.CompletableFuture;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import org.junit.jupiter.api.Test;

class CursorStateTest {
  @Test
  void cachesOneItemAndClosesWithoutCancellationAtNaturalEnd() {
    var nextCalls = new AtomicInteger();
    var cancelCalls = new AtomicInteger();
    var closeCalls = new AtomicInteger();
    var cursor =
        new CursorState<>(
            new CursorBackend<String>() {
              @Override
              public String next() {
                return nextCalls.getAndIncrement() == 0 ? "one" : null;
              }

              @Override
              public void cancel() {
                cancelCalls.incrementAndGet();
              }

              @Override
              public void close() {
                closeCalls.incrementAndGet();
              }
            });

    assertTrue(cursor.hasNext());
    assertTrue(cursor.hasNext());
    assertEquals(1, nextCalls.get());
    assertEquals("one", cursor.next());
    assertFalse(cursor.hasNext());
    assertEquals(2, nextCalls.get());
    assertEquals(0, cancelCalls.get());
    assertEquals(1, closeCalls.get());
  }

  @Test
  void explicitCloseCancelsAndClosesOnlyOnce() {
    var cancelCalls = new AtomicInteger();
    var closeCalls = new AtomicInteger();
    var cursor =
        new CursorState<>(
            new CursorBackend<String>() {
              @Override
              public String next() {
                return null;
              }

              @Override
              public void cancel() {
                cancelCalls.incrementAndGet();
              }

              @Override
              public void close() {
                closeCalls.incrementAndGet();
              }
            });

    cursor.close();
    cursor.close();

    assertEquals(1, cancelCalls.get());
    assertEquals(1, closeCalls.get());
  }

  @Test
  void closeWakesAnActiveBoundaryCallWithoutWaitingForIt() throws Exception {
    var entered = new CountDownLatch(1);
    var cancelled = new CountDownLatch(1);
    var cursor =
        new CursorState<>(
            new CursorBackend<String>() {
              @Override
              public String next() {
                entered.countDown();
                try {
                  cancelled.await();
                } catch (InterruptedException error) {
                  Thread.currentThread().interrupt();
                  throw new PdfException.RuntimeError("Interrupted", error);
                }
                throw new PdfException.Cancelled("Operation cancelled", null);
              }

              @Override
              public void cancel() {
                cancelled.countDown();
              }

              @Override
              public void close() {}
            });
    CompletableFuture<Boolean> activeCall = CompletableFuture.supplyAsync(cursor::hasNext);
    assertTrue(entered.await(1, TimeUnit.SECONDS));

    CompletableFuture<Void> closeCall = CompletableFuture.runAsync(cursor::close);
    closeCall.get(1, TimeUnit.SECONDS);

    ExecutionException failure =
        org.junit.jupiter.api.Assertions.assertThrows(
            ExecutionException.class, () -> activeCall.get(1, TimeUnit.SECONDS));
    assertInstanceOf(PdfException.Cancelled.class, failure.getCause());
  }
}
