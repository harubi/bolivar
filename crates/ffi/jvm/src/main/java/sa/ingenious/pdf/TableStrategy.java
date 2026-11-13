package sa.ingenious.pdf;

import java.util.Locale;

/** How table extraction finds edges on one axis (pdfplumber's strategies). */
public enum TableStrategy {
  LINES("lines"),
  LINES_STRICT("lines_strict"),
  TEXT("text"),
  EXPLICIT("explicit");

  private final String value;

  TableStrategy(String value) {
    this.value = value;
  }

  /** The pdfplumber name, e.g. {@code lines_strict}. */
  public String value() {
    return value;
  }

  /** The strategy for a pdfplumber name. */
  public static TableStrategy parse(String value) {
    for (TableStrategy strategy : values()) {
      if (strategy.value.equals(value.toLowerCase(Locale.ROOT))) {
        return strategy;
      }
    }
    throw new IllegalArgumentException(
        "Unknown table strategy " + value + "; expected lines, lines_strict, text or explicit");
  }
}
