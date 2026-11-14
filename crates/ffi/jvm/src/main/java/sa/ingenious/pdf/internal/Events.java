package sa.ingenious.pdf.internal;

/**
 * Flight Recorder spans for opens and batches. They cost nothing unless a recording enables them,
 * and nothing at all on a runtime without the {@code jdk.jfr} module (a minimal jlink image): then
 * every span is a shared no-op and the event classes are never loaded.
 */
final class Events {
  private Events() {}

  /** A document open in progress. */
  interface OpenSpan {
    void end(String source, long bytes);
  }

  /** A native batch in progress: produced, then copied into Java. */
  interface BatchSpan {
    void end(String kind, long elements);
  }

  private static final boolean JFR = ModuleLayer.boot().findModule("jdk.jfr").isPresent();

  private static final OpenSpan NO_OPEN = (source, bytes) -> {};
  private static final BatchSpan NO_BATCH = (kind, elements) -> {};

  static OpenSpan open() {
    return JFR ? JfrEvents.open() : NO_OPEN;
  }

  static BatchSpan batch() {
    return JFR ? JfrEvents.batch() : NO_BATCH;
  }
}
