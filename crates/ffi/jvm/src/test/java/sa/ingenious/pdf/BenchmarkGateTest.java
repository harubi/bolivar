package sa.ingenious.pdf;

import static org.junit.jupiter.api.Assertions.assertTrue;

import java.lang.management.ManagementFactory;
import java.util.Arrays;
import java.util.function.LongSupplier;
import org.junit.jupiter.api.Assumptions;
import org.junit.jupiter.api.Tag;
import org.junit.jupiter.api.Test;

/**
 * Guards the binding's cost on a committed 50-page fixture. Gates use measures that hold on any
 * machine: layout time relative to plain text time on the same run (the native layout work is
 * shared, so the gap is the binding), and JVM bytes allocated per character. Run with {@code
 * ./gradlew benchmark}; it prints every measurement.
 */
@Tag("benchmark")
class BenchmarkGateTest {
  private static final String DOCUMENT = Fixtures.path("nonfree/i1040nr.pdf");
  private static final int RUNS = 9;

  /** Layout may take at most this much longer than text extraction. */
  private static final double MAX_LAYOUT_TO_TEXT = 1.25;

  /** JVM bytes the layout cursor may allocate per character. */
  private static final double MAX_LAYOUT_BYTES_PER_CHAR = 86;

  /** JVM bytes the raw cursor may allocate per character. */
  private static final double MAX_RAW_BYTES_PER_CHAR = 275;

  private static final com.sun.management.ThreadMXBean THREADS =
      (com.sun.management.ThreadMXBean) ManagementFactory.getThreadMXBean();

  private static long fastest(LongSupplier run) {
    // Each result is used, so the JIT cannot drop the work.
    long total = 0;
    for (int i = 0; i < 3; i++) {
      total += run.getAsLong();
    }
    long[] times = new long[RUNS];
    for (int i = 0; i < RUNS; i++) {
      long start = System.nanoTime();
      total += run.getAsLong();
      times[i] = System.nanoTime() - start;
    }
    assertTrue(total > 0, "every run reads something");
    return Arrays.stream(times).min().orElseThrow();
  }

  private static long allocated(Runnable run) {
    run.run();
    long before = THREADS.getCurrentThreadAllocatedBytes();
    run.run();
    return THREADS.getCurrentThreadAllocatedBytes() - before;
  }

  private static long text() {
    try (var document = Document.open(DOCUMENT)) {
      return document.text().length();
    }
  }

  private static long layout() {
    long chars = 0;
    try (var document = Document.open(DOCUMENT);
        var pages = document.pages()) {
      while (pages.hasNext()) {
        chars += pages.next().glyphCount();
      }
    }
    return chars;
  }

  private static long raw() {
    long chars = 0;
    try (var document = Document.open(DOCUMENT);
        var pages = document.rawPages()) {
      while (pages.hasNext()) {
        chars += pages.next().glyphCount();
      }
    }
    return chars;
  }

  @Test
  void bindingCostStaysWithinBudget() {
    long chars = layout();
    long textNanos = fastest(BenchmarkGateTest::text);
    long layoutNanos = fastest(BenchmarkGateTest::layout);
    long rawNanos = fastest(BenchmarkGateTest::raw);
    double ratio = (double) layoutNanos / textNanos;
    double layoutBytes = (double) allocated(BenchmarkGateTest::layout) / chars;
    double rawBytes = (double) allocated(BenchmarkGateTest::raw) / chars;
    System.out.printf(
        "benchmark: %d chars; text %.1f ms, layout %.1f ms (x%.2f), raw %.1f ms;"
            + " layout %.1f B/char, raw %.1f B/char%n",
        chars, textNanos / 1e6, layoutNanos / 1e6, ratio, rawNanos / 1e6, layoutBytes, rawBytes);
    assertTrue(ratio <= MAX_LAYOUT_TO_TEXT, "layout/text " + ratio);
    assertTrue(layoutBytes <= MAX_LAYOUT_BYTES_PER_CHAR, "layout B/char " + layoutBytes);
    assertTrue(rawBytes <= MAX_RAW_BYTES_PER_CHAR, "raw B/char " + rawBytes);
  }

  /**
   * Wall-clock report, without gates, for the documents named in {@code -Dbolivar.bench.documents}
   * (comma separated). The nightly workflow passes larger reference PDFs here.
   */
  @Test
  void reportExtraDocuments() {
    String documents = System.getProperty("bolivar.bench.documents", "");
    Assumptions.assumeFalse(documents.isBlank(), "no extra documents");
    for (String path : documents.split(",", -1)) {
      LongSupplier text =
          () -> {
            try (var document = Document.open(path)) {
              return document.text().length();
            }
          };
      LongSupplier layout =
          () -> {
            long chars = 0;
            try (var document = Document.open(path);
                var pages = document.pages()) {
              while (pages.hasNext()) {
                chars += pages.next().glyphCount();
              }
            }
            return chars;
          };
      System.out.printf(
          "report: %s; text %.1f ms, layout %.1f ms%n",
          path, fastest(text) / 1e6, fastest(layout) / 1e6);
    }
  }
}
