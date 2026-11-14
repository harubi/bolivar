package sa.ingenious.pdf.internal;

import static java.lang.foreign.ValueLayout.ADDRESS;
import static java.lang.foreign.ValueLayout.JAVA_BYTE;
import static java.lang.foreign.ValueLayout.JAVA_CHAR;
import static java.lang.foreign.ValueLayout.JAVA_DOUBLE;
import static java.lang.foreign.ValueLayout.JAVA_INT;
import static java.lang.foreign.ValueLayout.JAVA_LONG;

import java.lang.foreign.MemorySegment;
import org.jspecify.annotations.Nullable;

/**
 * One batch copied into Java arrays. The native batch is freed as soon as it is read, so no native
 * memory outlives the call that produced it. Not part of the public API.
 *
 * <p>Text stays a {@code char[]}; strings are cut from it only when asked for. Ranges into it are
 * {@code int} start/end pairs, {@code -1} for absent. Strings that repeat (fonts, glyphs, colour
 * spaces, tags) come from a per-batch table, decoded once.
 */
@SuppressWarnings("restricted") // Restricted FFM calls are confined to this package.
public final class Columns {
  private static final int MAGIC = 0x424F_4C56;
  private static final long HEADER_SIZE = Contract.BATCH_HEADER.byteSize();
  private static final long DESC_SIZE = Contract.COLUMN_DESC.byteSize();
  private static final long KIND = Contract.offset(Contract.BATCH_HEADER, "kind");
  private static final long NCOLS = Contract.offset(Contract.BATCH_HEADER, "ncols");
  private static final long COLS = Contract.offset(Contract.BATCH_HEADER, "cols");
  private static final long ABI = Contract.offset(Contract.BATCH_HEADER, "abi_version");
  private static final long PTR = Contract.offset(Contract.COLUMN_DESC, "ptr");
  private static final long LEN = Contract.offset(Contract.COLUMN_DESC, "len");
  private static final long ELEM = Contract.offset(Contract.COLUMN_DESC, "elem");

  private final Object[] columns;
  private final char[] text;
  private final String[] strings;
  private final Schema.Kind kind;
  private final long elements;

  private Columns(Schema.Kind kind, Object[] columns, String[] strings, long elements) {
    this.kind = kind;
    this.columns = columns;
    this.text = (char[]) columns[Schema.TEXT];
    this.strings = strings;
    this.elements = elements;
  }

  /** Read the batch at {@code batch}, check it is of {@code kind}, and free it. */
  static Columns readAndFree(MemorySegment batch, Schema.Kind kind) {
    if (batch.address() == 0) {
      throw new IllegalStateException("native call returned no batch");
    }
    try {
      return read(batch.reinterpret(HEADER_SIZE), kind);
    } finally {
      Native.batchFree(batch);
    }
  }

  private static Columns read(MemorySegment header, Schema.Kind kind) {
    int magic = header.get(JAVA_INT, 0);
    int abi = header.get(JAVA_INT, ABI);
    int actualKind = header.get(JAVA_INT, KIND);
    int ncols = header.get(JAVA_INT, NCOLS);
    if (magic != MAGIC || abi != Native.ABI_VERSION) {
      throw new IllegalStateException("not a Bolivar batch of ABI " + Native.ABI_VERSION);
    }
    if (actualKind != kind.number() || ncols != kind.columnCount()) {
      throw new IllegalStateException(
          "expected batch kind "
              + kind.number()
              + " with "
              + kind.columnCount()
              + " columns, got kind "
              + actualKind
              + " with "
              + ncols);
    }
    MemorySegment dir = header.get(ADDRESS, COLS).reinterpret(ncols * DESC_SIZE);
    Object[] columns = new Object[ncols];
    long elements = 0;
    for (int i = 0; i < ncols; i++) {
      long base = i * DESC_SIZE;
      Schema.Col column = kind.column(i);
      Schema.Elem elem = Schema.Elem.of(dir.get(JAVA_INT, base + ELEM));
      if (elem != column.elem()) {
        throw new IllegalStateException(
            "column " + column.name() + " is " + elem + ", expected " + column.elem());
      }
      long len = dir.get(JAVA_LONG, base + LEN);
      columns[i] = copy(dir.get(ADDRESS, base + PTR), len, elem);
      elements += len;
    }
    int ints = ((long[]) columns[Schema.INTS]).length;
    int floats = ((double[]) columns[Schema.FLOATS]).length;
    if (ints != kind.intCount() || floats != kind.floatCount()) {
      throw new IllegalStateException("batch kind " + kind + " has the wrong scalar slots");
    }
    char[] chars = (char[]) columns[Schema.STRINGS];
    int[] offsets = (int[]) columns[Schema.STRING_OFF];
    String[] strings = new String[Math.max(offsets.length - 1, 0)];
    for (int i = 0; i < strings.length; i++) {
      strings[i] = new String(chars, offsets[i], offsets[i + 1] - offsets[i]);
    }
    return new Columns(kind, columns, strings, elements);
  }

  private static Object copy(MemorySegment ptr, long len, Schema.Elem elem) {
    if (len < 0 || len > Integer.MAX_VALUE - 8) {
      throw new IllegalStateException("column length out of range: " + len);
    }
    int n = (int) len;
    if (n == 0) {
      return switch (elem) {
        case F64 -> new double[0];
        case I32 -> new int[0];
        case I64 -> new long[0];
        case U8 -> new byte[0];
        case U16 -> new char[0];
      };
    }
    return switch (elem) {
      case F64 -> ptr.reinterpret(n * 8L).toArray(JAVA_DOUBLE);
      case I32 -> ptr.reinterpret(n * 4L).toArray(JAVA_INT);
      case I64 -> ptr.reinterpret(n * 8L).toArray(JAVA_LONG);
      case U8 -> ptr.reinterpret(n).toArray(JAVA_BYTE);
      case U16 -> ptr.reinterpret(n * 2L).toArray(JAVA_CHAR);
    };
  }

  /** The total element count of every column; for events. */
  long elements() {
    return elements;
  }

  private int index(Schema.Col column) {
    int index = column.index();
    if (index >= columns.length || !kind.column(index).equals(column)) {
      throw new IllegalArgumentException(column + " is not a column of " + kind);
    }
    return index;
  }

  public double[] f64(Schema.Col column) {
    return (double[]) columns[index(column)];
  }

  public int[] i32(Schema.Col column) {
    return (int[]) columns[index(column)];
  }

  public long[] i64(Schema.Col column) {
    return (long[]) columns[index(column)];
  }

  public byte[] u8(Schema.Col column) {
    return (byte[]) columns[index(column)];
  }

  /** An {@code Ints} slot; it must belong to this batch's kind. */
  public long integer(Enum<?> slot) {
    return ((long[]) columns[Schema.INTS])[kind.intSlot(slot)];
  }

  /** A {@code Floats} slot; it must belong to this batch's kind. */
  public double real(Enum<?> slot) {
    return ((double[]) columns[Schema.FLOATS])[kind.floatSlot(slot)];
  }

  /** The batch text; ranges index into it. Shared, not copied: do not change it. */
  public char[] text() {
    return text;
  }

  /** The text from {@code start} to {@code end}, or null when {@code start} is negative. */
  public @Nullable String range(int start, int end) {
    return start < 0 ? null : new String(text, start, end - start);
  }

  /** The text of the {@code index}th range of a range column. */
  public @Nullable String range(Schema.Col column, int index) {
    int[] ranges = i32(column);
    return range(ranges[2 * index], ranges[2 * index + 1]);
  }

  /** The per-batch string table. Shared, not copied: do not change it. */
  public String[] strings() {
    return strings;
  }

  /** An interned string, or null for a negative id. */
  public @Nullable String string(int id) {
    return id < 0 ? null : strings[id];
  }
}
