package sa.ingenious.pdf;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;

import org.junit.jupiter.api.Test;
import sa.ingenious.pdf.internal.Schema;

class CursorTest {
  @Test
  void aDecodeFailureEndsTheCursorInsteadOfSkippingAPage() {
    try (var document = Document.open(Fixtures.path("contrib/pagelabels.pdf"));
        Cursor<Integer> pages =
            document.cursor(
                Schema.Kind.SUMMARY,
                null,
                (columns, out) -> {
                  throw new IllegalStateException("bad page");
                })) {
      assertEquals(
          "bad page", assertThrows(IllegalStateException.class, pages::hasNext).getMessage());
      assertFalse(pages.hasNext(), "the cursor ends rather than reading the next page");
    }
  }
}
