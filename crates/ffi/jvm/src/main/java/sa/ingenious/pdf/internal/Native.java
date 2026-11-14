package sa.ingenious.pdf.internal;

import static java.lang.foreign.ValueLayout.ADDRESS;
import static java.lang.foreign.ValueLayout.JAVA_BYTE;
import static java.lang.foreign.ValueLayout.JAVA_CHAR;
import static java.lang.foreign.ValueLayout.JAVA_INT;
import static java.lang.foreign.ValueLayout.JAVA_LONG;

import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemoryLayout;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.SegmentAllocator;
import java.lang.foreign.SymbolLookup;
import java.lang.invoke.MethodHandle;
import java.nio.charset.StandardCharsets;
import org.jspecify.annotations.Nullable;
import sa.ingenious.pdf.PdfException;

/**
 * The native functions, bound once through FFM. Not part of the public API.
 *
 * <p>The method handles are {@code static final} fields of a holder class, so the JIT treats them
 * as constants, and a failed load surfaces as an exception on first use instead of poisoning every
 * class that mentions this one. {@link #ensureLoaded} checks the ABI version, the library version,
 * every batch schema and every hand-mirrored layout before the holder is touched.
 */
@SuppressWarnings("restricted") // Restricted FFM calls are confined to this package.
public final class Native {
  private Native() {}

  /** The ABI version this jar speaks; must equal {@code bolivar_abi_version()}. */
  public static final int ABI_VERSION = 2;

  private static final long ERR_FIELD = Contract.offset(Contract.ERROR_OUT, "field");
  private static final long ERR_FIELD_LEN = Contract.offset(Contract.ERROR_OUT, "field_len");
  private static final long ERR_MESSAGE = Contract.offset(Contract.ERROR_OUT, "message");
  private static final long ERR_MESSAGE_LEN = Contract.offset(Contract.ERROR_OUT, "message_len");

  /** Room for the out-slots and the {@code ErrorOut} of any one call. */
  private static final long FRAME_BYTES = 4 * ADDRESS.byteSize() + Contract.ERROR_OUT.byteSize();

  private static final class Lib {
    private Lib() {}

    static MethodHandle bind(String name, @Nullable MemoryLayout result, MemoryLayout... args) {
      return Native.bind(NativeLoader.lookup(), name, result, args);
    }

    static final MethodHandle VERSION = bind("bolivar_version", null, ADDRESS, ADDRESS);
    static final MethodHandle SCHEMA =
        bind("bolivar_schema", JAVA_INT, JAVA_INT, ADDRESS, ADDRESS, ADDRESS);
    static final MethodHandle DESCRIBE =
        bind("bolivar_describe", JAVA_INT, ADDRESS, JAVA_LONG, ADDRESS, ADDRESS, ADDRESS);
    static final MethodHandle LIVE_HANDLES = bind("bolivar_debug_live_handles", JAVA_LONG);
    static final MethodHandle ERROR_FREE = bind("bolivar_error_free", null, ADDRESS);
    static final MethodHandle OPEN_PATH =
        bind("bolivar_open_path", JAVA_INT, ADDRESS, JAVA_LONG, ADDRESS, ADDRESS, ADDRESS);
    static final MethodHandle BUFFER_ALLOC =
        bind("bolivar_buffer_alloc", JAVA_INT, JAVA_LONG, ADDRESS, ADDRESS, ADDRESS);
    static final MethodHandle BUFFER_GROW =
        bind("bolivar_buffer_grow", JAVA_INT, ADDRESS, JAVA_LONG, JAVA_LONG, ADDRESS, ADDRESS);
    static final MethodHandle OPEN_BUFFER =
        bind("bolivar_open_buffer", JAVA_INT, ADDRESS, JAVA_LONG, ADDRESS, ADDRESS, ADDRESS);
    static final MethodHandle BUFFER_FREE = bind("bolivar_buffer_free", null, ADDRESS);
    static final MethodHandle DOCUMENT_FREE = bind("bolivar_document_free", null, ADDRESS);
    static final MethodHandle DOCUMENT_TEXT =
        bind("bolivar_document_text", JAVA_INT, ADDRESS, ADDRESS, ADDRESS);
    static final MethodHandle DOCUMENT_METADATA =
        bind("bolivar_document_metadata", JAVA_INT, ADDRESS, ADDRESS, ADDRESS);
    static final MethodHandle DOCUMENT_RAW_PAGE =
        bind("bolivar_document_raw_page", JAVA_INT, ADDRESS, JAVA_LONG, ADDRESS, ADDRESS);
    static final MethodHandle DOCUMENT_COUNTS =
        bind("bolivar_document_counts", JAVA_INT, ADDRESS, ADDRESS, ADDRESS, ADDRESS);
    static final MethodHandle CURSOR_OPEN =
        bind("bolivar_cursor_open", JAVA_INT, ADDRESS, JAVA_INT, ADDRESS, ADDRESS, ADDRESS);
    static final MethodHandle CURSOR_NEXT =
        bind("bolivar_cursor_next", JAVA_INT, ADDRESS, ADDRESS, ADDRESS);
    static final MethodHandle CURSOR_CANCEL = bind("bolivar_cursor_cancel", null, ADDRESS);
    static final MethodHandle CURSOR_FREE = bind("bolivar_cursor_free", null, ADDRESS);
    static final MethodHandle BATCH_FREE = bind("bolivar_batch_free", null, ADDRESS);
  }

  private static MethodHandle bind(
      SymbolLookup lookup, String name, @Nullable MemoryLayout result, MemoryLayout... args) {
    var symbol =
        lookup
            .find(name)
            .orElseThrow(() -> new UnsatisfiedLinkError("missing native symbol " + name));
    var descriptor =
        result == null ? FunctionDescriptor.ofVoid(args) : FunctionDescriptor.of(result, args);
    return Linker.nativeLinker().downcallHandle(symbol, descriptor);
  }

  private static volatile boolean verified;

  /**
   * Load the library and check that it matches this jar, once. Until that succeeds every call
   * retries and fails with the real cause, rather than a {@code NoClassDefFoundError}.
   */
  public static void ensureLoaded() {
    if (verified) {
      return;
    }
    synchronized (Native.class) {
      if (verified) {
        return;
      }
      // The ABI is checked through a one-off handle, so a library of another ABI never
      // initialises the holder, whose symbols it may lack.
      int abi;
      try {
        abi = (int) bind(NativeLoader.lookup(), "bolivar_abi_version", JAVA_INT).invokeExact();
      } catch (Throwable error) {
        throw new IllegalStateException("Cannot read the Bolivar native ABI version", error);
      }
      if (abi != ABI_VERSION) {
        throw new IllegalStateException(
            "Bolivar native ABI " + abi + " does not match the jar's ABI " + ABI_VERSION);
      }
      String nativeVersion = version();
      String jarVersion = NativeLoader.jarVersion();
      if (!nativeVersion.equals(jarVersion)) {
        throw new IllegalStateException(
            "Bolivar version mismatch: JVM " + jarVersion + ", native " + nativeVersion);
      }
      Schema.verify(Native::schema);
      Contract.verify(Native::describe);
      verified = true;
    }
  }

  // ---- calls ------------------------------------------------------------------------------

  /** A status-returning native call, given the {@code ErrorOut} to fill. */
  @FunctionalInterface
  interface Call {
    int invoke(MemorySegment err) throws Throwable;
  }

  /** A native call that returns nothing. */
  @FunctionalInterface
  interface VoidCall {
    void invoke() throws Throwable;
  }

  /** Space for one call's out-slots and {@code ErrorOut}, carved from a confined arena. */
  private static SegmentAllocator frame(Arena arena) {
    return SegmentAllocator.slicingAllocator(arena.allocate(FRAME_BYTES, 8));
  }

  private static MemorySegment errorOut(SegmentAllocator allocator) {
    return allocator.allocate(Contract.ERROR_OUT);
  }

  /** Run a status-returning call and throw {@link PdfException} on failure. */
  private static void call(MemorySegment err, Call call) {
    int status;
    try {
      status = call.invoke(err);
    } catch (RuntimeException | Error error) {
      throw error;
    } catch (Throwable error) {
      throw new IllegalStateException("native call failed", error);
    }
    if (status != 0) {
      throw failure(status, err);
    }
  }

  /** Run a void native call. */
  private static void run(VoidCall call) {
    try {
      call.invoke();
    } catch (RuntimeException | Error error) {
      throw error;
    } catch (Throwable error) {
      throw new IllegalStateException("native call failed", error);
    }
  }

  private static PdfException failure(int status, MemorySegment err) {
    try {
      String field = utf16(err.get(ADDRESS, ERR_FIELD), err.get(JAVA_LONG, ERR_FIELD_LEN));
      String message = utf16(err.get(ADDRESS, ERR_MESSAGE), err.get(JAVA_LONG, ERR_MESSAGE_LEN));
      return PdfException.of(status, message, field);
    } finally {
      run(() -> {
        Lib.ERROR_FREE.invokeExact(err);
      });
    }
  }

  /** Copy {@code len} UTF-16 units at {@code ptr}; null for a null pointer. */
  private static @Nullable String utf16(MemorySegment ptr, long len) {
    if (ptr.address() == 0) {
      return null;
    }
    return new String(ptr.reinterpret(Math.multiplyExact(len, 2L)).toArray(JAVA_CHAR));
  }

  /** Copy {@code len} UTF-8 bytes at {@code ptr}. */
  private static String utf8(MemorySegment ptr, long len) {
    if (len == 0) {
      return "";
    }
    return new String(ptr.reinterpret(len).toArray(JAVA_BYTE), StandardCharsets.UTF_8);
  }

  // ---- library ----------------------------------------------------------------------------

  /** The native library's version. */
  public static String version() {
    try (var arena = Arena.ofConfined()) {
      var frame = frame(arena);
      var ptr = frame.allocate(ADDRESS);
      var len = frame.allocate(JAVA_LONG);
      run(() -> {
        Lib.VERSION.invokeExact(ptr, len);
      });
      return utf8(ptr.get(ADDRESS, 0), len.get(JAVA_LONG, 0));
    }
  }

  /** The native schema text of a batch kind. */
  public static String schema(int kind) {
    try (var arena = Arena.ofConfined()) {
      var frame = frame(arena);
      var ptr = frame.allocate(ADDRESS);
      var len = frame.allocate(JAVA_LONG);
      call(errorOut(frame), e -> (int) Lib.SCHEMA.invokeExact(kind, ptr, len, e));
      return utf8(ptr.get(ADDRESS, 0), len.get(JAVA_LONG, 0));
    }
  }

  /** The native description of something this jar mirrors: a struct layout or named values. */
  public static String describe(String name) {
    try (var arena = Arena.ofConfined()) {
      byte[] bytes = name.getBytes(StandardCharsets.UTF_8);
      var nameSegment = arena.allocateFrom(JAVA_BYTE, bytes);
      var frame = frame(arena);
      var ptr = frame.allocate(ADDRESS);
      var len = frame.allocate(JAVA_LONG);
      call(
          errorOut(frame),
          e -> (int) Lib.DESCRIBE.invokeExact(nameSegment, (long) bytes.length, ptr, len, e));
      return utf8(ptr.get(ADDRESS, 0), len.get(JAVA_LONG, 0));
    }
  }

  /** Live native handles of every kind; for leak tests. */
  public static long liveHandles() {
    try {
      return (long) Lib.LIVE_HANDLES.invokeExact();
    } catch (Throwable error) {
      throw new IllegalStateException(error);
    }
  }

  // ---- documents --------------------------------------------------------------------------

  /** Open a file by path. */
  public static MemorySegment openPath(String path, MemorySegment options) {
    var event = Events.open();
    try (var arena = Arena.ofConfined()) {
      byte[] bytes = path.getBytes(StandardCharsets.UTF_8);
      var pathSegment = arena.allocateFrom(JAVA_BYTE, bytes);
      var frame = frame(arena);
      var out = frame.allocate(ADDRESS);
      call(
          errorOut(frame),
          e -> (int) Lib.OPEN_PATH.invokeExact(pathSegment, (long) bytes.length, options, out, e));
      return out.get(ADDRESS, 0);
    } finally {
      event.end("path", 0);
    }
  }

  /** A Rust-owned buffer and where to write into it. */
  public record Buffer(MemorySegment handle, MemorySegment data) {}

  /** Allocate a buffer of at least {@code capacity} bytes. */
  public static Buffer bufferAlloc(long capacity) {
    try (var arena = Arena.ofConfined()) {
      var frame = frame(arena);
      var out = frame.allocate(ADDRESS);
      var data = frame.allocate(ADDRESS);
      call(errorOut(frame), e -> (int) Lib.BUFFER_ALLOC.invokeExact(capacity, out, data, e));
      return new Buffer(out.get(ADDRESS, 0), data.get(ADDRESS, 0).reinterpret(capacity));
    }
  }

  /** Grow a buffer whose first {@code filled} bytes are written; the data may move. */
  public static Buffer bufferGrow(MemorySegment buffer, long filled, long capacity) {
    try (var arena = Arena.ofConfined()) {
      var frame = frame(arena);
      var data = frame.allocate(ADDRESS);
      call(
          errorOut(frame),
          e -> (int) Lib.BUFFER_GROW.invokeExact(buffer, filled, capacity, data, e));
      return new Buffer(buffer, data.get(ADDRESS, 0).reinterpret(capacity));
    }
  }

  /** Open a buffer's first {@code filled} bytes. Consumes the buffer, even on failure. */
  public static MemorySegment openBuffer(MemorySegment buffer, long filled, MemorySegment options) {
    var event = Events.open();
    try (var arena = Arena.ofConfined()) {
      var frame = frame(arena);
      var out = frame.allocate(ADDRESS);
      call(
          errorOut(frame), e -> (int) Lib.OPEN_BUFFER.invokeExact(buffer, filled, options, out, e));
      return out.get(ADDRESS, 0);
    } finally {
      event.end("buffer", filled);
    }
  }

  /** Free a buffer that was never opened. */
  public static void bufferFree(MemorySegment buffer) {
    run(() -> {
      Lib.BUFFER_FREE.invokeExact(buffer);
    });
  }

  /** Free a document. */
  public static void documentFree(MemorySegment document) {
    run(() -> {
      Lib.DOCUMENT_FREE.invokeExact(document);
    });
  }

  /** Read and free the batch an out-slot holds, recording an event. */
  private static Columns batch(Events.BatchSpan event, MemorySegment out, Schema.Kind kind) {
    Columns columns = Columns.readAndFree(out.get(ADDRESS, 0), kind);
    event.end(kind.name(), columns.elements());
    return columns;
  }

  /** The whole document's text, as a text batch. */
  public static Columns documentText(MemorySegment document) {
    var event = Events.batch();
    try (var arena = Arena.ofConfined()) {
      var frame = frame(arena);
      var out = frame.allocate(ADDRESS);
      call(errorOut(frame), e -> (int) Lib.DOCUMENT_TEXT.invokeExact(document, out, e));
      return batch(event, out, Schema.Kind.TEXT);
    }
  }

  /** The document's metadata, as a metadata batch. */
  public static Columns documentMetadata(MemorySegment document) {
    var event = Events.batch();
    try (var arena = Arena.ofConfined()) {
      var frame = frame(arena);
      var out = frame.allocate(ADDRESS);
      call(errorOut(frame), e -> (int) Lib.DOCUMENT_METADATA.invokeExact(document, out, e));
      return batch(event, out, Schema.Kind.METADATA);
    }
  }

  /** One 1-based page, as a raw batch. */
  public static Columns documentRawPage(MemorySegment document, long pageNumber) {
    var event = Events.batch();
    try (var arena = Arena.ofConfined()) {
      var frame = frame(arena);
      var out = frame.allocate(ADDRESS);
      call(
          errorOut(frame),
          e -> (int) Lib.DOCUMENT_RAW_PAGE.invokeExact(document, pageNumber, out, e));
      return batch(event, out, Schema.Kind.RAW);
    }
  }

  /** The declared page count and the number of pages the options select. */
  public static long[] documentCounts(MemorySegment document) {
    try (var arena = Arena.ofConfined()) {
      var frame = frame(arena);
      var declared = frame.allocate(JAVA_LONG);
      var selected = frame.allocate(JAVA_LONG);
      call(
          errorOut(frame),
          e -> (int) Lib.DOCUMENT_COUNTS.invokeExact(document, declared, selected, e));
      return new long[] {declared.get(JAVA_LONG, 0), selected.get(JAVA_LONG, 0)};
    }
  }

  // ---- cursors ----------------------------------------------------------------------------

  /** Open a cursor of {@code kind} over a document. */
  public static MemorySegment cursorOpen(
      MemorySegment document, Schema.Kind kind, MemorySegment tableOptions) {
    int number = kind.number();
    try (var arena = Arena.ofConfined()) {
      var frame = frame(arena);
      var out = frame.allocate(ADDRESS);
      call(
          errorOut(frame),
          e -> (int) Lib.CURSOR_OPEN.invokeExact(document, number, tableOptions, out, e));
      return out.get(ADDRESS, 0);
    }
  }

  /**
   * Per-cursor scratch: an out-slot and an {@code ErrorOut}, reused by every {@link #cursorNext}
   * call of one cursor. Its arena is automatic: closing a shared arena per cursor would cost a
   * JVM-wide handshake, and calls may come from different threads.
   */
  public record Scratch(MemorySegment out, MemorySegment err) {
    public static Scratch allocate() {
      var allocator = SegmentAllocator.slicingAllocator(Arena.ofAuto().allocate(FRAME_BYTES, 8));
      return new Scratch(allocator.allocate(ADDRESS), errorOut(allocator));
    }
  }

  /** The next batch of a cursor, or null at its end. One call at a time per scratch. */
  public static @Nullable Columns cursorNext(
      MemorySegment cursor, Scratch scratch, Schema.Kind kind) {
    var event = Events.batch();
    MemorySegment out = scratch.out();
    call(scratch.err(), e -> (int) Lib.CURSOR_NEXT.invokeExact(cursor, out, e));
    return out.get(ADDRESS, 0).address() == 0 ? null : batch(event, out, kind);
  }

  /** Ask a cursor to stop. Safe beside a running {@link #cursorNext}. */
  public static void cursorCancel(MemorySegment cursor) {
    run(() -> {
      Lib.CURSOR_CANCEL.invokeExact(cursor);
    });
  }

  /** Free a cursor. */
  public static void cursorFree(MemorySegment cursor) {
    run(() -> {
      Lib.CURSOR_FREE.invokeExact(cursor);
    });
  }

  static void batchFree(MemorySegment batch) {
    run(() -> {
      Lib.BATCH_FREE.invokeExact(batch);
    });
  }
}
