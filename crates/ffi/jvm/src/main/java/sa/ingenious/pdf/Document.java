package sa.ingenious.pdf;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.ref.Reference;
import java.nio.channels.Channels;
import java.nio.channels.ReadableByteChannel;
import java.nio.file.Path;
import java.util.Objects;
import java.util.function.Function;
import org.jspecify.annotations.Nullable;
import sa.ingenious.pdf.internal.Native;
import sa.ingenious.pdf.internal.NativeLoader;
import sa.ingenious.pdf.internal.NativeResource;
import sa.ingenious.pdf.internal.Schema;

/**
 * An open PDF document. Use it in try-with-resources; a document that is never closed is freed by a
 * {@link java.lang.ref.Cleaner}.
 *
 * <p>Calls may come from several threads at once. {@link #close()} waits for calls in progress.
 * Cursors keep working after their document closes; a file opened by path stays mapped (and locked,
 * on Windows) until they close too.
 *
 * <p>The JVM must allow native access: pass {@code --enable-native-access=ALL-UNNAMED}, or the
 * module name {@code sa.ingenious.bolivar} on the module path.
 */
public final class Document implements AutoCloseable {
  private static final long INITIAL_STREAM_BUFFER = 1 << 16;
  private static final long MAX_READ = 1 << 30;

  private final NativeResource resource;

  private Document(MemorySegment handle) {
    this.resource = new NativeResource(this, handle, Native::documentFree);
  }

  // ---- opening ------------------------------------------------------------------------------

  /** Open a file by path. The file is memory-mapped: keep it unchanged while it is open. */
  public static Document open(String path) {
    return open(path, Options.DEFAULT);
  }

  /** Open a file by path. The file is memory-mapped: keep it unchanged while it is open. */
  public static Document open(String path, Options options) {
    Objects.requireNonNull(path, "path");
    Native.ensureLoaded();
    try (var arena = Arena.ofConfined()) {
      return new Document(Native.openPath(path, NativeStructs.options(options, arena)));
    }
  }

  public static Document open(Path path) {
    return open(path, Options.DEFAULT);
  }

  public static Document open(Path path, Options options) {
    return open(path.toString(), options);
  }

  public static Document open(File file) {
    return open(file, Options.DEFAULT);
  }

  public static Document open(File file, Options options) {
    return open(file.getPath(), options);
  }

  /** Open PDF bytes. They are copied once into native memory. */
  public static Document open(byte[] bytes) {
    return open(bytes, Options.DEFAULT);
  }

  public static Document open(byte[] bytes, Options options) {
    Objects.requireNonNull(bytes, "bytes");
    return openCopy(MemorySegment.ofArray(bytes), options);
  }

  /**
   * Open PDF bytes held in a memory segment. They are copied once, so the segment may be freed as
   * soon as this returns.
   */
  public static Document open(MemorySegment bytes, Options options) {
    Objects.requireNonNull(bytes, "bytes");
    return openCopy(bytes, options);
  }

  /** Read a stream to its end, straight into native memory, then close it. */
  public static Document open(InputStream stream) {
    return open(stream, Options.DEFAULT);
  }

  public static Document open(InputStream stream, Options options) {
    Objects.requireNonNull(stream, "stream");
    Native.ensureLoaded();
    try (var arena = Arena.ofConfined()) {
      // Built before the buffer, so a failure here cannot leak it.
      MemorySegment nativeOptions = NativeStructs.options(options, arena);
      return readStream(stream, nativeOptions);
    }
  }

  private static Document readStream(InputStream stream, MemorySegment nativeOptions) {
    Native.Buffer buffer = Native.bufferAlloc(INITIAL_STREAM_BUFFER);
    long filled = 0;
    boolean owned = true;
    try (stream;
        ReadableByteChannel channel = Channels.newChannel(stream)) {
      while (true) {
        if (filled == buffer.data().byteSize()) {
          buffer =
              Native.bufferGrow(
                  buffer.handle(), filled, Math.multiplyExact(buffer.data().byteSize(), 2L));
        }
        // A ByteBuffer holds at most Integer.MAX_VALUE bytes, so read in bounded chunks.
        long room = Math.min(buffer.data().byteSize() - filled, MAX_READ);
        int read = channel.read(buffer.data().asSlice(filled, room).asByteBuffer());
        if (read < 0) {
          break;
        }
        filled += read;
      }
      owned = false;
      return new Document(Native.openBuffer(buffer.handle(), filled, nativeOptions));
    } catch (IOException error) {
      throw new PdfException.IoError(String.valueOf(error.getMessage()), error);
    } finally {
      if (owned) {
        Native.bufferFree(buffer.handle());
      }
    }
  }

  private static Document openCopy(MemorySegment bytes, Options options) {
    Native.ensureLoaded();
    try (var arena = Arena.ofConfined()) {
      // Built before the buffer, so a failure here cannot leak it.
      MemorySegment nativeOptions = NativeStructs.options(options, arena);
      long size = bytes.byteSize();
      Native.Buffer buffer = Native.bufferAlloc(Math.max(size, 1));
      try {
        MemorySegment.copy(bytes, 0, buffer.data(), 0, size);
      } catch (RuntimeException | Error error) {
        Native.bufferFree(buffer.handle());
        throw error;
      }
      return new Document(Native.openBuffer(buffer.handle(), size, nativeOptions));
    }
  }

  // ---- one-shot conveniences ----------------------------------------------------------------

  /** Open, extract the whole text, and close. */
  public static String extractText(String path) {
    return extractText(path, Options.DEFAULT);
  }

  public static String extractText(String path, Options options) {
    try (var document = open(path, options)) {
      return document.text();
    }
  }

  public static String extractText(Path path) {
    return extractText(path.toString(), Options.DEFAULT);
  }

  public static String extractText(Path path, Options options) {
    return extractText(path.toString(), options);
  }

  public static String extractText(File file) {
    return extractText(file.getPath(), Options.DEFAULT);
  }

  public static String extractText(File file, Options options) {
    return extractText(file.getPath(), options);
  }

  public static String extractText(byte[] bytes) {
    return extractText(bytes, Options.DEFAULT);
  }

  public static String extractText(byte[] bytes, Options options) {
    try (var document = open(bytes, options)) {
      return document.text();
    }
  }

  public static String extractText(InputStream stream) {
    return extractText(stream, Options.DEFAULT);
  }

  public static String extractText(InputStream stream, Options options) {
    try (var document = open(stream, options)) {
      return document.text();
    }
  }

  /** The native library's version; equal to the jar's. */
  public static String version() {
    Native.ensureLoaded();
    return Native.version();
  }

  /** The version this jar was built as; does not load the native library. */
  public static String jarVersion() {
    return NativeLoader.jarVersion();
  }

  // ---- reads --------------------------------------------------------------------------------

  private <R> R read(Function<MemorySegment, R> call) {
    long stamp = resource.acquire();
    if (stamp == 0) {
      throw new IllegalStateException("The document is closed");
    }
    try {
      return call.apply(resource.handle());
    } finally {
      resource.release(stamp);
      Reference.reachabilityFence(this);
    }
  }

  /** Package-private for tests that need their own decoder. */
  <T> Cursor<T> cursor(Schema.Kind kind, @Nullable TableOptions tables, Cursor.Decoder<T> decoder) {
    return read(handle -> {
      try (var arena = Arena.ofConfined()) {
        MemorySegment nativeTables =
            tables == null ? MemorySegment.NULL : NativeStructs.tableOptions(tables, arena);
        MemorySegment cursor = Native.cursorOpen(handle, kind, nativeTables, MemorySegment.NULL);
        try {
          return new Cursor<>(cursor, kind, decoder);
        } catch (RuntimeException | Error error) {
          // Not yet owned by a Cursor, so nothing else would free it.
          Native.cursorFree(cursor);
          throw error;
        }
      }
    });
  }

  /** The whole document's text, pages separated by form feeds. */
  public String text() {
    return read(handle -> Decode.text(Native.documentText(handle)));
  }

  public Metadata metadata() {
    return read(handle -> Decode.metadata(Native.documentMetadata(handle)));
  }

  /** The pages in the file, as the document declares them. */
  public int declaredPageCount() {
    return read(handle -> Math.toIntExact(Native.documentCounts(handle)[0]));
  }

  /** The pages this document's options select. */
  public int pageCount() {
    return read(handle -> Math.toIntExact(Native.documentCounts(handle)[1]));
  }

  /** One 1-based page with nothing dropped. */
  public RawPage rawPage(int pageNumber) {
    return read(handle -> new RawPage(Native.documentRawPage(handle, pageNumber)));
  }

  /** The laid-out pages, one at a time. */
  public Cursor<LayoutPage> pages() {
    return cursor(Schema.Kind.LAYOUT, null, (c, out) -> out.accept(new LayoutPage(c)));
  }

  /** Every page with nothing dropped, one at a time. */
  public Cursor<RawPage> rawPages() {
    return cursor(Schema.Kind.RAW, null, (c, out) -> out.accept(new RawPage(c)));
  }

  /**
   * The text, fonts and digit styles of each page. Only distinct (font, text, size) glyphs cross
   * from native code, not every glyph.
   */
  public Cursor<PageText> pageTexts() {
    return cursor(Schema.Kind.PAGE_TEXT, null, (c, out) -> out.accept(Decode.pageText(c)));
  }

  public Cursor<PageSummary> pageSummaries() {
    return cursor(Schema.Kind.SUMMARY, null, (c, out) -> out.accept(Decode.summary(c)));
  }

  public Cursor<Table> tables() {
    return tables(TableOptions.DEFAULT);
  }

  /** Every table of every page, one at a time. */
  public Cursor<Table> tables(TableOptions options) {
    return this.<Table>cursor(Schema.Kind.TABLES, options, Decode::tables);
  }

  public Cursor<PageTableRows> tableRows() {
    return tableRows(TableOptions.DEFAULT);
  }

  /** The raw table rows of each page, as the pdfplumber-compatible pipeline emits them. */
  public Cursor<PageTableRows> tableRows(TableOptions options) {
    return cursor(Schema.Kind.TABLE_ROWS, options, (c, out) -> out.accept(Decode.tableRows(c)));
  }

  /** Free the document once calls in progress finish. Idempotent. */
  @Override
  public void close() {
    try {
      resource.close();
    } finally {
      Reference.reachabilityFence(this);
    }
  }
}
