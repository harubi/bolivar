package sa.ingenious.pdf;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static sa.ingenious.pdf.Fixtures.fixture;
import static sa.ingenious.pdf.Fixtures.path;

import java.io.ByteArrayInputStream;
import java.io.FilterInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.nio.file.Files;
import java.util.ArrayList;
import java.util.List;
import java.util.NoSuchElementException;
import org.junit.jupiter.api.Test;

class DocumentTest {
  private static final String SIMPLE = path("simple1.pdf");
  private static final String NICS = path("pdfplumber/nics-background-checks-2015-11.pdf");
  private static final String LABELS = path("contrib/pagelabels.pdf");

  @Test
  void everySourceReadsTheSameText() throws IOException {
    String expected = Document.extractText(SIMPLE);
    assertTrue(expected.contains("Hello"));
    byte[] bytes = Files.readAllBytes(fixture("simple1.pdf"));
    assertEquals(expected, Document.extractText(fixture("simple1.pdf")));
    assertEquals(expected, Document.extractText(fixture("simple1.pdf").toFile()));
    assertEquals(expected, Document.extractText(bytes));
    assertEquals(expected, Document.extractText(new ByteArrayInputStream(bytes)));
    try (var arena = Arena.ofConfined();
        var document = Document.open(arena.allocateFrom(java.lang.foreign.ValueLayout.JAVA_BYTE, bytes), Options.DEFAULT)) {
      assertEquals(expected, document.text());
    }
  }

  @Test
  void streamsGrowPastTheInitialBufferAndAreClosed() throws IOException {
    byte[] bytes = Files.readAllBytes(fixture("nonfree/dmca.pdf"));
    assertTrue(bytes.length > (1 << 16), "fixture larger than the first buffer");
    var closed = new boolean[1];
    // Small reads force many grow steps.
    InputStream stream =
        new FilterInputStream(new ByteArrayInputStream(bytes)) {
          @Override
          public int read(byte[] b, int off, int len) throws IOException {
            return super.read(b, off, Math.min(len, 1000));
          }

          @Override
          public void close() throws IOException {
            closed[0] = true;
            super.close();
          }
        };
    try (var document = Document.open(stream)) {
      assertEquals(Document.extractText(bytes), document.text());
    }
    assertTrue(closed[0]);
  }

  @Test
  void streamFailuresBecomeIoErrors() {
    InputStream failing =
        new InputStream() {
          @Override
          public int read() throws IOException {
            throw new IOException("disk on fire");
          }
        };
    var error = assertThrows(PdfException.IoError.class, () -> Document.open(failing));
    assertEquals("disk on fire", error.getMessage());
  }

  @Test
  void failuresCarryCodesMessagesAndFields() {
    var missing = assertThrows(PdfException.IoNotFound.class, () -> Document.open(path("nope.pdf")));
    assertEquals(3, missing.code());
    assertFalse(missing.getMessage().isBlank());

    assertThrows(PdfException.InvalidPath.class, () -> Document.open(" "));

    var wrong = Options.builder().password("bar").build();
    assertThrows(PdfException.EncryptionError.class, () -> Document.open(path("encryption/rc4-40.pdf"), wrong));
    var right = Options.builder().password("foo").build();
    try (var document = Document.open(path("encryption/rc4-40.pdf"), right)) {
      assertTrue(document.text().contains("Secret!"));
    }

    var zero = Options.builder().pages(0).build();
    var page = assertThrows(PdfException.InvalidArgument.class, () -> Document.open(SIMPLE, zero));
    assertEquals("pages", page.field());

    var flow =
        Options.builder().layout(LayoutOptions.builder().boxesFlow(1.5).build()).build();
    assertEquals(
        "boxes_flow",
        assertThrows(PdfException.InvalidArgument.class, () -> Document.open(SIMPLE, flow)).field());

    try (var document = Document.open(SIMPLE)) {
      assertEquals(
          "page_number",
          assertThrows(PdfException.InvalidArgument.class, () -> document.rawPage(0)).field());
      var negative = TableOptions.builder().snapTolerance(-1).build();
      assertEquals(
          "snap_tolerance",
          assertThrows(PdfException.InvalidArgument.class, () -> document.tables(negative)).field());
    }
  }

  @Test
  void closedDocumentsRefuseCallsAndCloseTwice() {
    var document = Document.open(SIMPLE);
    document.close();
    document.close();
    assertThrows(IllegalStateException.class, document::text);
    assertThrows(IllegalStateException.class, document::pages);
  }

  @Test
  void optionsSelectPagesAndCounts() {
    var options = Options.builder().pages(2, 4).build();
    try (var document = Document.open(LABELS, options);
        var pages = document.pages()) {
      assertEquals(5, document.declaredPageCount());
      assertEquals(2, document.pageCount());
      List<Integer> numbers = new ArrayList<>();
      pages.forEachRemaining(page -> numbers.add(page.pageNumber()));
      assertEquals(List.of(2, 4), numbers);
    }
    try (var document = Document.open(LABELS)) {
      assertEquals("iv", document.rawPage(2).label());
    }
  }

  @Test
  void cursorsYieldEveryShape() {
    try (var document = Document.open(NICS)) {
      try (var pages = document.pages()) {
        LayoutPage page = pages.next();
        assertFalse(pages.hasNext());
        assertThrows(NoSuchElementException.class, pages::next);
        assertEquals(4319, page.charCount());
        assertEquals(page.charAt(0), page.textBoxes().get(0).lines().get(0).chars().get(0));
        assertEquals(page.charFontName(0), page.charFontNames()[0]);
        double[] copy = page.charX0s();
        copy[0] = -1;
        assertNotEquals(-1, page.charX0(0), "column getters copy");
      }
      try (var raw = document.rawPages()) {
        RawPage page = raw.next();
        assertEquals(1, page.tables().size());
        assertEquals("DeviceGray", page.characterAt(0).nonStrokingColorSpace());
        assertEquals(6, page.characterAt(0).matrix().size());
      }
      try (var tables = document.tables()) {
        Table table = tables.next();
        assertEquals(1, table.pageNumber());
        assertEquals(table.rowCount(), table.toGrid().size());
      }
      try (var rows = document.tableRows()) {
        PageTableRows page = rows.next();
        assertEquals(1, page.tableCount());
        assertEquals(page.cells().size(), page.rowOffsets()[page.rowOffsets().length - 1]);
      }
      try (var texts = document.pageTexts()) {
        PageText text = texts.next();
        assertFalse(text.fonts().isEmpty());
      }
      try (var summaries = document.pageSummaries()) {
        assertEquals(1, summaries.next().pageNumber());
      }
      Metadata metadata = document.metadata();
      assertEquals(1, metadata.pageCount());
      assertEquals("none", metadata.form());
      assertNull(metadata.title());
    }
  }

  @Test
  void streamsCloseTheirCursorAndEarlyCloseStops() {
    try (var document = Document.open(path("nonfree/dmca.pdf"))) {
      long count;
      try (var stream = document.pages().stream()) {
        count = stream.limit(3).count();
      }
      assertEquals(3, count);
      var pages = document.pages();
      pages.next();
      pages.close();
      assertFalse(pages.hasNext());
      pages.cancel();
    }
  }

  @Test
  void disablingBoxesFlowChangesTheLayout() {
    var none = Options.builder().layout(LayoutOptions.builder().disableBoxesFlow().build()).build();
    String paper = path("nonfree/naacl06-shinyama.pdf");
    try (var defaults = Document.open(paper);
        var disabled = Document.open(paper, none);
        var a = defaults.pages();
        var b = disabled.pages()) {
      assertNotEquals(a.next().text(), b.next().text());
    }
  }
}
