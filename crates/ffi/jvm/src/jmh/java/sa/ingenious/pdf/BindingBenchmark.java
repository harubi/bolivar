package sa.ingenious.pdf;

import java.nio.file.Path;
import java.util.concurrent.TimeUnit;
import org.openjdk.jmh.annotations.Benchmark;
import org.openjdk.jmh.annotations.BenchmarkMode;
import org.openjdk.jmh.annotations.Fork;
import org.openjdk.jmh.annotations.Measurement;
import org.openjdk.jmh.annotations.Mode;
import org.openjdk.jmh.annotations.OutputTimeUnit;
import org.openjdk.jmh.annotations.Scope;
import org.openjdk.jmh.annotations.Setup;
import org.openjdk.jmh.annotations.State;
import org.openjdk.jmh.annotations.TearDown;
import org.openjdk.jmh.annotations.Warmup;
import org.openjdk.jmh.infra.Blackhole;

/**
 * The binding's cost on the committed 50-page fixture: plain text, layout pages and raw pages from
 * one open document. Run with {@code ./gradlew jmh}; add {@code -prof gc} through {@code
 * jmh.profilers} for allocation rates.
 */
@State(Scope.Benchmark)
@BenchmarkMode(Mode.AverageTime)
@OutputTimeUnit(TimeUnit.MILLISECONDS)
@Warmup(iterations = 3, time = 2)
@Measurement(iterations = 5, time = 2)
@Fork(1)
public class BindingBenchmark {
  private Document document;

  @Setup
  public void open() {
    String repo = System.getProperty("bolivar.test.repo", "../../..");
    document = Document.open(Path.of(repo, "crates/core/tests/fixtures/nonfree/i1040nr.pdf"));
  }

  @TearDown
  public void close() {
    document.close();
  }

  @Benchmark
  public String text() {
    return document.text();
  }

  @Benchmark
  public void layout(Blackhole sink) {
    try (var pages = document.pages()) {
      pages.forEachRemaining(sink::consume);
    }
  }

  @Benchmark
  public void layoutGlyphs(Blackhole sink) {
    try (var pages = document.pages()) {
      while (pages.hasNext()) {
        for (Glyph glyph : pages.next().glyphs()) {
          sink.consume(glyph.x0());
        }
      }
    }
  }

  @Benchmark
  public void raw(Blackhole sink) {
    try (var pages = document.rawPages()) {
      pages.forEachRemaining(sink::consume);
    }
  }
}
