package sa.ingenious.pdf;

import java.util.Locale;

/** How table extraction finds edges on one axis (pdfplumber's strategies). */
public enum TableStrategy {
  LINES,
  LINES_STRICT,
  TEXT,
  EXPLICIT;

  /** The pdfplumber name, e.g. {@code lines_strict}: the constant's name in lower case. */
  public String value() {
    return name().toLowerCase(Locale.ROOT);
  }

  /** The strategy for a pdfplumber name. */
  public static TableStrategy parse(String value) {
    String wanted = value.toLowerCase(Locale.ROOT);
    for (TableStrategy strategy : values()) {
      if (strategy.value().equals(wanted)) {
        return strategy;
      }
    }
    throw new IllegalArgumentException(
        "Unknown table strategy " + value + "; expected lines, lines_strict, text or explicit");
  }
}
