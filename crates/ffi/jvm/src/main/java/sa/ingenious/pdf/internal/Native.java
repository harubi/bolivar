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
import java.lang.foreign.StructLayout;
import java.lang.foreign.SymbolLookup;
import java.lang.invoke.MethodHandle;
import java.nio.charset.StandardCharsets;
import sa.ingenious.pdf.PdfException;

/**
 * The native functions, bound once through FFM. Not part of the public API.
 *
 * <p>Handles live in a holder class, so a failed load surfaces as an exception on first use
 * instead of poisoning every class that mentions this one.
 */
@SuppressWarnings("restricted") // Restricted FFM calls are confined to this package.
public final class Native {
  private Native() {}

  /** The ABI version this jar speaks; must equal {@code bolivar_abi_version()}. */
  public static final int ABI_VERSION = 1;

  /** {@code ErrorOut}: code, padding, field (UTF-16 ptr, len), message (UTF-16 ptr, len). */
  public static final StructLayout ERROR_OUT =
      MemoryLayout.structLayout(
          JAVA_INT.withName("code"),
          JAVA_INT.withName("pad"),
          ADDRESS.withName("field"),
          JAVA_LONG.withName("field_len"),
          ADDRESS.withName("message"),
          JAVA_LONG.withName("message_len"));

  /** The batch header: magic, ABI version, kind, column count, column directory. */
  public static final StructLayout BATCH_HEADER =
      MemoryLayout.structLayout(
          JAVA_INT.withName("magic"),
          JAVA_INT.withName("abi_version"),
          JAVA_INT.withName("kind"),
          JAVA_INT.withName("ncols"),
          ADDRESS.withName("cols"));

  /** One column directory entry: data pointer, element count, element type. */
  public static final StructLayout COLUMN_DESC =
      MemoryLayout.structLayout(
          ADDRESS.withName("ptr"),
          JAVA_LONG.withName("len"),
          JAVA_INT.withName("elem"),
          JAVA_INT.withName("pad"));

  private static final class Lib {

    static MethodHandle bind(String name, MemoryLayout result, MemoryLayout... args) {
      return Native.bind(NativeLoader.lookup(), name, result, args);
    }

    static final MethodHandle ABI = bind("bolivar_abi_version", JAVA_INT);
    static final MethodHandle VERSION = bind("bolivar_version", null, ADDRESS, ADDRESS);
    static final MethodHandle SCHEMA =
        bind("bolivar_schema", JAVA_INT, JAVA_INT, ADDRESS, ADDRESS, ADDRESS);
    static final MethodHandle STRUCT_SIZES =
        bind("bolivar_debug_struct_sizes", JAVA_LONG, ADDRESS, JAVA_LONG);
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
        bind("bolivar_cursor_open", JAVA_INT, ADDRESS, JAVA_INT, ADDRESS, ADDRESS, ADDRESS, ADDRESS);
    static final MethodHandle CURSOR_NEXT =
        bind("bolivar_cursor_next", JAVA_INT, ADDRESS, ADDRESS, ADDRESS);
    static final MethodHandle CANCEL = bind("bolivar_cancel", null, ADDRESS);
    static final MethodHandle CURSOR_FREE = bind("bolivar_cursor_free", null, ADDRESS);
    static final MethodHandle CANCEL_FREE = bind("bolivar_cancel_free", null, ADDRESS);
    static final MethodHandle BATCH_FREE = bind("bolivar_batch_free", null, ADDRESS);
  }

  private static MethodHandle bind(
      SymbolLookup lookup, String name, MemoryLayout result, MemoryLayout... args) {
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
   * Load the library and check its ABI and version, once. Until it succeeds, every call retries
   * and fails with the real cause, rather than a {@code NoClassDefFoundError}.
   */
  public static void ensureLoaded() {
    if (verified) {
      return;
    }
    synchronized (Native.class) {
      if (verified) {
        return;
      }
      // Check through one-off handles, so a mismatch never poisons the handle holder and every
      // retry reports the real cause.
      SymbolLookup lookup = NativeLoader.lookup();
      int abi;
      try {
        abi = (int) bind(lookup, "bolivar_abi_version", JAVA_INT).invokeExact();
      } catch (Throwable error) {
        throw new IllegalStateException("Cannot read the Bolivar native ABI version", error);
      }
      if (abi != ABI_VERSION) {
        throw new IllegalStateException(
            "Bolivar native ABI " + abi + " does not match the jar's ABI " + ABI_VERSION);
      }
      String nativeVersion = version(bind(lookup, "bolivar_version", null, ADDRESS, ADDRESS));
      String jarVersion = NativeLoader.jarVersion();
      if (!nativeVersion.equals(jarVersion)) {
        throw new IllegalStateException(
            "Bolivar version mismatch: JVM " + jarVersion + ", native " + nativeVersion);
      }
      verified = true;
    }
  }

  // ---- calls ------------------------------------------------------------------------------

  @FunctionalInterface
  public interface Call {
    int invoke(MemorySegment err) throws Throwable;
  }

  @FunctionalInterface
  public interface VoidCall {
    void invoke() throws Throwable;
  }

  /** Allocate a fresh, zeroed {@code ErrorOut}. */
  public static MemorySegment errorOut(SegmentAllocator allocator) {
    return allocator.allocate(ERROR_OUT);
  }

  /** Run a status-returning call and throw {@link PdfException} on failure. */
  public static void call(MemorySegment err, Call call) {
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
  public static void run(VoidCall call) {
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
      String field = utf16(err.get(ADDRESS, 8), err.get(JAVA_LONG, 16));
      String message = utf16(err.get(ADDRESS, 24), err.get(JAVA_LONG, 32));
      return PdfException.of(status, message, field);
    } finally {
      run(() -> {
      Lib.ERROR_FREE.invokeExact(err);
    });
    }
  }

  /** Copy {@code len} UTF-16 units at {@code ptr}; null for a null pointer. */
  public static String utf16(MemorySegment ptr, long len) {
    if (ptr.address() == 0) {
      return null;
    }
    char[] chars = ptr.reinterpret(Math.multiplyExact(len, 2L)).toArray(JAVA_CHAR);
    return new String(chars);
  }

  /** Copy {@code len} UTF-8 bytes at {@code ptr}. */
  public static String utf8(MemorySegment ptr, long len) {
    byte[] bytes = ptr.reinterpret(len).toArray(JAVA_BYTE);
    return new String(bytes, StandardCharsets.UTF_8);
  }

  // ---- typed wrappers ---------------------------------------------------------------------

  public static String version() {
    return version(Lib.VERSION);
  }

  private static String version(MethodHandle handle) {
    try (var arena = Arena.ofConfined()) {
      var ptr = arena.allocate(ADDRESS);
      var len = arena.allocate(JAVA_LONG);
      run(() -> {
        handle.invokeExact(ptr, len);
      });
      return utf8(ptr.get(ADDRESS, 0), len.get(JAVA_LONG, 0));
    }
  }

  public static int abiVersion() {
    try {
      return (int) Lib.ABI.invokeExact();
    } catch (Throwable error) {
      throw new IllegalStateException(error);
    }
  }

  /** The schema text of a batch kind: {@code Name:ELEM} lines. */
  public static String schema(int kind) {
    try (var arena = Arena.ofConfined()) {
      var ptr = arena.allocate(ADDRESS);
      var len = arena.allocate(JAVA_LONG);
      var err = errorOut(arena);
      call(err, e -> (int) Lib.SCHEMA.invokeExact(kind, ptr, len, e));
      return utf8(ptr.get(ADDRESS, 0), len.get(JAVA_LONG, 0));
    }
  }

  /** The native sizes of Options, TableOptions, ErrorOut, BatchHeader and ColumnDesc. */
  public static long[] structSizes() {
    try (var arena = Arena.ofConfined()) {
      var out = arena.allocate(JAVA_LONG, 5);
      try {
        long count = (long) Lib.STRUCT_SIZES.invokeExact(out, 5L);
        return out.asSlice(0, Math.min(count, 5) * 8).toArray(JAVA_LONG);
      } catch (Throwable error) {
        throw new IllegalStateException(error);
      }
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

  public static MemorySegment openPath(String path, MemorySegment options) {
    try (var arena = Arena.ofConfined()) {
      var bytes = path.getBytes(StandardCharsets.UTF_8);
      var pathSegment = arena.allocateFrom(JAVA_BYTE, bytes);
      var out = arena.allocate(ADDRESS);
      var err = errorOut(arena);
      call(
          err,
          e ->
              (int)
                  Lib.OPEN_PATH.invokeExact(pathSegment, (long) bytes.length, options, out, e));
      return out.get(ADDRESS, 0);
    }
  }

  /** A Rust-owned buffer and where to write into it. */
  public record Buffer(MemorySegment handle, MemorySegment data) {}

  public static Buffer bufferAlloc(long capacity) {
    try (var arena = Arena.ofConfined()) {
      var out = arena.allocate(ADDRESS);
      var data = arena.allocate(ADDRESS);
      var err = errorOut(arena);
      call(err, e -> (int) Lib.BUFFER_ALLOC.invokeExact(capacity, out, data, e));
      return new Buffer(out.get(ADDRESS, 0), data.get(ADDRESS, 0).reinterpret(capacity));
    }
  }

  public static Buffer bufferGrow(MemorySegment buffer, long filled, long capacity) {
    try (var arena = Arena.ofConfined()) {
      var data = arena.allocate(ADDRESS);
      var err = errorOut(arena);
      call(err, e -> (int) Lib.BUFFER_GROW.invokeExact(buffer, filled, capacity, data, e));
      return new Buffer(buffer, data.get(ADDRESS, 0).reinterpret(capacity));
    }
  }

  /** Open a buffer's first {@code filled} bytes. Consumes the buffer, even on failure. */
  public static MemorySegment openBuffer(MemorySegment buffer, long filled, MemorySegment options) {
    try (var arena = Arena.ofConfined()) {
      var out = arena.allocate(ADDRESS);
      var err = errorOut(arena);
      call(err, e -> (int) Lib.OPEN_BUFFER.invokeExact(buffer, filled, options, out, e));
      return out.get(ADDRESS, 0);
    }
  }

  public static void bufferFree(MemorySegment buffer) {
    run(() -> {
      Lib.BUFFER_FREE.invokeExact(buffer);
    });
  }

  public static void documentFree(MemorySegment document) {
    run(() -> {
      Lib.DOCUMENT_FREE.invokeExact(document);
    });
  }

  /** A batch-returning document call; reads and frees the batch. */
  private static Columns documentBatch(MethodHandle handle, MemorySegment document, int kind) {
    try (var arena = Arena.ofConfined()) {
      var out = arena.allocate(ADDRESS);
      var err = errorOut(arena);
      call(err, e -> (int) handle.invokeExact(document, out, e));
      return Columns.readAndFree(out.get(ADDRESS, 0), kind);
    }
  }

  public static Columns documentText(MemorySegment document) {
    return documentBatch(Lib.DOCUMENT_TEXT, document, Schema.TEXT);
  }

  public static Columns documentMetadata(MemorySegment document) {
    return documentBatch(Lib.DOCUMENT_METADATA, document, Schema.METADATA);
  }

  public static Columns documentRawPage(MemorySegment document, long pageNumber) {
    try (var arena = Arena.ofConfined()) {
      var out = arena.allocate(ADDRESS);
      var err = errorOut(arena);
      call(err, e -> (int) Lib.DOCUMENT_RAW_PAGE.invokeExact(document, pageNumber, out, e));
      return Columns.readAndFree(out.get(ADDRESS, 0), Schema.RAW);
    }
  }

  /** The declared page count and the number of pages the options select. */
  public static long[] documentCounts(MemorySegment document) {
    try (var arena = Arena.ofConfined()) {
      var declared = arena.allocate(JAVA_LONG);
      var selected = arena.allocate(JAVA_LONG);
      var err = errorOut(arena);
      call(err, e -> (int) Lib.DOCUMENT_COUNTS.invokeExact(document, declared, selected, e));
      return new long[] {declared.get(JAVA_LONG, 0), selected.get(JAVA_LONG, 0)};
    }
  }

  /** A cursor and the handle that cancels it. */
  public record CursorHandles(MemorySegment cursor, MemorySegment cancel) {}

  public static CursorHandles cursorOpen(
      MemorySegment document, int kind, MemorySegment tableOptions) {
    try (var arena = Arena.ofConfined()) {
      var out = arena.allocate(ADDRESS);
      var cancel = arena.allocate(ADDRESS);
      var err = errorOut(arena);
      call(
          err,
          e -> (int) Lib.CURSOR_OPEN.invokeExact(document, kind, tableOptions, out, cancel, e));
      return new CursorHandles(out.get(ADDRESS, 0), cancel.get(ADDRESS, 0));
    }
  }

  /**
   * The next batch of a cursor, or null at the end. {@code scratch} holds an out-pointer slot
   * followed by an {@code ErrorOut}, reused across calls under the cursor's lock.
   */
  public static Columns cursorNext(MemorySegment cursor, MemorySegment scratch, int kind) {
    var out = scratch.asSlice(0, ADDRESS.byteSize());
    var err = scratch.asSlice(ADDRESS.byteSize(), ERROR_OUT.byteSize());
    call(err, e -> (int) Lib.CURSOR_NEXT.invokeExact(cursor, out, e));
    MemorySegment batch = out.get(ADDRESS, 0);
    return batch.address() == 0 ? null : Columns.readAndFree(batch, kind);
  }

  /** Scratch space for {@link #cursorNext}. */
  public static MemorySegment cursorScratch(SegmentAllocator allocator) {
    return allocator.allocate(ADDRESS.byteSize() + ERROR_OUT.byteSize(), 8);
  }

  public static void cancel(MemorySegment cancel) {
    run(() -> {
      Lib.CANCEL.invokeExact(cancel);
    });
  }

  public static void cursorFree(MemorySegment cursor) {
    run(() -> {
      Lib.CURSOR_FREE.invokeExact(cursor);
    });
  }

  public static void cancelFree(MemorySegment cancel) {
    run(() -> {
      Lib.CANCEL_FREE.invokeExact(cancel);
    });
  }

  static void batchFree(MemorySegment batch) {
    run(() -> {
      Lib.BATCH_FREE.invokeExact(batch);
    });
  }
}
