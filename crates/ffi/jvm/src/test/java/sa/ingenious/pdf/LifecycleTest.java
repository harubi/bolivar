package sa.ingenious.pdf;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.fail;
import static sa.ingenious.pdf.Fixtures.path;

import java.time.Duration;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;
import sa.ingenious.pdf.internal.Native;

/** Native handles under concurrency, early close, cancellation and garbage collection. */
class LifecycleTest {
  private static final String BIG = path("nonfree/i1040nr.pdf");
  private static final String SMALL = path("simple1.pdf");

  @BeforeAll
  static void load() {
    Native.ensureLoaded();
  }

  /** Wait until the live handle count returns to {@code expected}, collecting garbage. */
  private static void awaitHandles(long expected) throws InterruptedException {
    long deadline = System.nanoTime() + Duration.ofSeconds(20).toNanos();
    while (Native.liveHandles() != expected) {
      if (System.nanoTime() > deadline) {
        fail("live handles stayed at " + Native.liveHandles() + ", expected " + expected);
      }
      System.gc();
      Thread.sleep(20);
    }
  }

  @Test
  void cancelFromAnotherThreadStopsIteration() throws Exception {
    long before = Native.liveHandles();
    try (var document = Document.open(BIG);
        var pages = document.pages()) {
      var firstPage = new CountDownLatch(1);
      var pool = Executors.newSingleThreadExecutor();
      Future<Integer> reader =
          pool.submit(
              () -> {
                int count = 0;
                try {
                  while (pages.hasNext()) {
                    pages.next();
                    count++;
                    firstPage.countDown();
                  }
                } catch (PdfException.Cancelled cancelled) {
                  return -count;
                }
                return count;
              });
      assertTrue(firstPage.await(30, TimeUnit.SECONDS));
      pages.cancel();
      int result = reader.get(30, TimeUnit.SECONDS);
      // Either the cursor reported the cancel, or it had already read every page.
      assertTrue(result < 0 || result == document.pageCount(), "result " + result);
      pool.shutdown();
    }
    awaitHandles(before);
  }

  @Test
  void closeRacesCancelWithoutUseAfterFree() throws Exception {
    long before = Native.liveHandles();
    try (var document = Document.open(BIG)) {
      var pool = Executors.newFixedThreadPool(4);
      for (int round = 0; round < 200; round++) {
        var pages = document.pages();
        var stop = new AtomicBoolean();
        var running = new CountDownLatch(3);
        List<Future<?>> cancellers = new ArrayList<>();
        for (int t = 0; t < 3; t++) {
          cancellers.add(
              pool.submit(
                  () -> {
                    running.countDown();
                    while (!stop.get()) {
                      pages.cancel();
                    }
                  }));
        }
        assertTrue(running.await(10, TimeUnit.SECONDS));
        pages.close();
        stop.set(true);
        for (Future<?> canceller : cancellers) {
          canceller.get(10, TimeUnit.SECONDS);
        }
      }
      pool.shutdown();
    }
    awaitHandles(before);
  }

  @Test
  void closeDuringIterationEndsIt() throws Exception {
    try (var document = Document.open(BIG)) {
      var pages = document.pages();
      var started = new CountDownLatch(1);
      var pool = Executors.newSingleThreadExecutor();
      Future<Integer> reader =
          pool.submit(
              () -> {
                int count = 0;
                while (pages.hasNext()) {
                  pages.next();
                  count++;
                  started.countDown();
                }
                return count;
              });
      assertTrue(started.await(30, TimeUnit.SECONDS));
      pages.close();
      // A close ends iteration quietly, not with Cancelled.
      assertTrue(reader.get(30, TimeUnit.SECONDS) >= 1);
      pool.shutdown();
    }
  }

  @Test
  void documentCloseWaitsForCallsInProgress() throws Exception {
    long before = Native.liveHandles();
    var pool = Executors.newFixedThreadPool(4);
    for (int round = 0; round < 50; round++) {
      var document = Document.open(SMALL);
      var failures = new AtomicInteger();
      var completed = new AtomicInteger();
      var reading = new CountDownLatch(3);
      List<Future<?>> readers = new ArrayList<>();
      for (int t = 0; t < 3; t++) {
        readers.add(
            pool.submit(
                () -> {
                  for (int i = 0; i < 20; i++) {
                    if (i == 1) {
                      reading.countDown();
                    }
                    try {
                      assertTrue(document.text().contains("Hello"));
                      completed.incrementAndGet();
                    } catch (IllegalStateException closed) {
                      return;
                    } catch (RuntimeException other) {
                      failures.incrementAndGet();
                    }
                  }
                }));
      }
      // Close while every reader is mid-loop: each has finished at least one call.
      assertTrue(reading.await(30, TimeUnit.SECONDS));
      document.close();
      for (Future<?> reader : readers) {
        reader.get(30, TimeUnit.SECONDS);
      }
      assertEquals(0, failures.get());
      assertTrue(completed.get() >= 3);
    }
    pool.shutdown();
    awaitHandles(before);
  }

  @Test
  void cursorsOutliveTheirDocument() {
    var document = Document.open(BIG);
    try (var pages = document.pages()) {
      document.close();
      int count = 0;
      while (pages.hasNext()) {
        pages.next();
        count++;
      }
      assertEquals(50, count);
    }
  }

  @Test
  void unreachableHandlesAreFreedByTheCleaner() throws InterruptedException {
    long before = Native.liveHandles();
    for (int i = 0; i < 50; i++) {
      var document = Document.open(SMALL);
      document.pages().next();
      // Neither the document nor the cursor is closed.
    }
    awaitHandles(before);
  }

  @Test
  void garbageCollectionDuringNativeCallsIsSafe() throws Exception {
    var stop = new AtomicBoolean();
    Thread collector =
        Thread.ofPlatform()
            .start(
                () -> {
                  while (!stop.get()) {
                    System.gc();
                    // Back-to-back full collections would starve the test thread.
                    try {
                      Thread.sleep(1);
                    } catch (InterruptedException interrupted) {
                      return;
                    }
                  }
                });
    try {
      for (int i = 0; i < 300; i++) {
        // The document is unreachable as soon as text() starts; the fence keeps it alive.
        assertTrue(Document.open(SMALL).text().contains("Hello"));
        assertEquals(1, Document.open(SMALL).pages().next().pageNumber());
      }
    } finally {
      stop.set(true);
      collector.join();
    }
  }

  @Test
  void tenThousandCursorsLeakNothing() throws InterruptedException {
    long before = Native.liveHandles();
    try (var document = Document.open(SMALL)) {
      for (int i = 0; i < 10_000; i++) {
        try (var pages = document.pageSummaries()) {
          pages.next();
        }
      }
    }
    awaitHandles(before);
  }

  @Test
  void virtualThreadsShareTheLibrary() throws Exception {
    long before = Native.liveHandles();
    String expected = Document.extractText(SMALL);
    try (var executor = Executors.newVirtualThreadPerTaskExecutor()) {
      List<Future<String>> results = new ArrayList<>();
      for (int i = 0; i < 200; i++) {
        results.add(
            executor.submit(
                () -> {
                  try (var document = Document.open(SMALL);
                      var pages = document.pages()) {
                    pages.next();
                    return document.text();
                  }
                }));
      }
      for (Future<String> result : results) {
        try {
          assertEquals(expected, result.get(60, TimeUnit.SECONDS));
        } catch (ExecutionException error) {
          throw new AssertionError(error.getCause());
        }
      }
    }
    awaitHandles(before);
  }
}
