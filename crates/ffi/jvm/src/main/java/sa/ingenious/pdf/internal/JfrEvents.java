package sa.ingenious.pdf.internal;

import jdk.jfr.Category;
import jdk.jfr.DataAmount;
import jdk.jfr.Description;
import jdk.jfr.Event;
import jdk.jfr.Label;
import jdk.jfr.Name;
import jdk.jfr.StackTrace;

/**
 * The JDK Flight Recorder events behind {@link Events}. Only {@link Events} touches this class, and
 * only when the {@code jdk.jfr} module is present, so a runtime without it never loads it.
 */
final class JfrEvents {
  private JfrEvents() {}

  static Events.OpenSpan open() {
    var event = new Open();
    event.begin();
    return event;
  }

  static Events.BatchSpan batch() {
    var event = new Batch();
    event.begin();
    return event;
  }

  /** A document open. */
  @Name("sa.ingenious.bolivar.Open")
  @Label("Bolivar Open")
  @Category("Bolivar")
  @Description("Opening a PDF document in native code")
  @StackTrace(false)
  static final class Open extends Event implements Events.OpenSpan {
    @Label("Source")
    @Description("path, buffer")
    String source = "";

    @Label("Size")
    @DataAmount
    long bytes;

    @Override
    public void end(String source, long bytes) {
      if (shouldCommit()) {
        this.source = source;
        this.bytes = bytes;
        commit();
      }
    }
  }

  /** One batch: a cursor's next page, or a one-shot document call. */
  @Name("sa.ingenious.bolivar.Batch")
  @Label("Bolivar Batch")
  @Category("Bolivar")
  @Description("Producing and copying one native batch")
  @StackTrace(false)
  static final class Batch extends Event implements Events.BatchSpan {
    @Label("Kind")
    String kind = "";

    @Label("Elements")
    @Description("Elements copied from native memory, across every column")
    long elements;

    @Override
    public void end(String kind, long elements) {
      if (shouldCommit()) {
        this.kind = kind;
        this.elements = elements;
        commit();
      }
    }
  }
}
