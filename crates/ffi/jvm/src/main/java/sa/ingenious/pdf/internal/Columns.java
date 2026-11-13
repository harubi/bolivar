package sa.ingenious.pdf.internal;

import static java.lang.foreign.ValueLayout.ADDRESS;
import static java.lang.foreign.ValueLayout.JAVA_BYTE;
import static java.lang.foreign.ValueLayout.JAVA_CHAR;
import static java.lang.foreign.ValueLayout.JAVA_DOUBLE;
import static java.lang.foreign.ValueLayout.JAVA_INT;
import static java.lang.foreign.ValueLayout.JAVA_LONG;

import java.lang.foreign.MemorySegment;

/**
 * One batch copied into Java arrays. The native batch is freed as soon as it is read, so no
 * native memory outlives the call that produced it. Not part of the public API.
 *
 * <p>Text is one string; ranges into it are {@code int} start/end pairs, {@code -1} for absent.
 * Strings that repeat (fonts, colour spaces, tags) are interned in a per-batch table.
 */
@SuppressWarnings("restricted") // Restricted FFM calls are confined to this package.
public final class Columns {
  private static final int MAGIC = 0x424F_4C56;

  private final int kind;
  private final Object[] columns;
  private final String text;
  private final String[] strings;

  private Columns(int kind, Object[] columns, String text, String[] strings) {
    this.kind = kind;
    this.columns = columns;
    this.text = text;
    this.strings = strings;
  }

  /** Read the batch at {@code batch}, check it is of {@code kind}, and free it. */
  static Columns readAndFree(MemorySegment batch, int kind) {
    if (batch.address() == 0) {
      throw new IllegalStateException("native call returned no batch");
    }
    try {
      return read(batch, kind);
    } finally {
      Native.batchFree(batch);
    }
  }

  private static Columns read(MemorySegment batch, int kind) {
    var header = batch.reinterpret(Native.BATCH_HEADER.byteSize());
    int magic = header.get(JAVA_INT, 0);
    int abi = header.get(JAVA_INT, 4);
    int actualKind = header.get(JAVA_INT, 8);
    int ncols = header.get(JAVA_INT, 12);
    if (magic != MAGIC || abi != Native.ABI_VERSION) {
      throw new IllegalStateException("not a Bolivar batch of ABI " + Native.ABI_VERSION);
    }
    Schema.Column[] schema = Schema.columns(kind);
    if (actualKind != kind || ncols != schema.length) {
      throw new IllegalStateException(
          "expected batch kind " + kind + " with " + schema.length + " columns, got kind "
              + actualKind + " with " + ncols);
    }
    var dir =
        header
            .get(ADDRESS, 16)
            .reinterpret(Math.multiplyExact((long) ncols, Native.COLUMN_DESC.byteSize()));
    long descSize = Native.COLUMN_DESC.byteSize();
    Object[] columns = new Object[ncols];
    for (int i = 0; i < ncols; i++) {
      long base = i * descSize;
      MemorySegment ptr = dir.get(ADDRESS, base);
      long len = dir.get(JAVA_LONG, base + 8);
      Schema.Elem elem = Schema.Elem.of(dir.get(JAVA_INT, base + 16));
      if (elem != schema[i].elem()) {
        throw new IllegalStateException(
            "column " + schema[i].name() + " is " + elem + ", expected " + schema[i].elem());
      }
      columns[i] = copy(ptr, len, elem);
    }
    String text = new String((char[]) columns[0]);
    char[] stringChars = (char[]) columns[1];
    int[] offsets = (int[]) columns[2];
    String[] strings = new String[Math.max(offsets.length - 1, 0)];
    for (int i = 0; i < strings.length; i++) {
      strings[i] = new String(stringChars, offsets[i], offsets[i + 1] - offsets[i]);
    }
    return new Columns(kind, columns, text, strings);
  }

  private static Object copy(MemorySegment ptr, long len, Schema.Elem elem) {
    if (len < 0 || len > Integer.MAX_VALUE - 8) {
      throw new IllegalStateException("column length out of range: " + len);
    }
    if (len == 0) {
      return switch (elem) {
        case F64 -> new double[0];
        case I32 -> new int[0];
        case I64 -> new long[0];
        case U8 -> new byte[0];
        case U16 -> new char[0];
      };
    }
    return switch (elem) {
      case F64 -> ptr.reinterpret(len * 8).toArray(JAVA_DOUBLE);
      case I32 -> ptr.reinterpret(len * 4).toArray(JAVA_INT);
      case I64 -> ptr.reinterpret(len * 8).toArray(JAVA_LONG);
      case U8 -> ptr.reinterpret(len).toArray(JAVA_BYTE);
      case U16 -> ptr.reinterpret(len * 2).toArray(JAVA_CHAR);
    };
  }

  public int kind() {
    return kind;
  }

  public double[] f64(Schema.Column column) {
    return (double[]) columns[column.ordinal()];
  }

  public int[] i32(Schema.Column column) {
    return (int[]) columns[column.ordinal()];
  }

  public long[] i64(Schema.Column column) {
    return (long[]) columns[column.ordinal()];
  }

  public byte[] u8(Schema.Column column) {
    return (byte[]) columns[column.ordinal()];
  }

  /** The batch's text; ranges index into it. */
  public String text() {
    return text;
  }

  /** The text of a range, or null for the absent range. */
  public String range(int start, int end) {
    return start < 0 ? null : text.substring(start, end);
  }

  /** The text of the {@code index}th range in a range column. */
  public String range(Schema.Column column, int index) {
    int[] ranges = i32(column);
    return range(ranges[2 * index], ranges[2 * index + 1]);
  }

  /** An interned string by id, or null for {@code -1}. */
  public String string(int id) {
    return id < 0 ? null : strings[id];
  }

  /** The per-batch string table. */
  public String[] strings() {
    return strings;
  }
}
