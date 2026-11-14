package sa.ingenious.pdf.internal;

import static java.lang.foreign.MemoryLayout.paddingLayout;
import static java.lang.foreign.MemoryLayout.sequenceLayout;
import static java.lang.foreign.MemoryLayout.structLayout;
import static java.lang.foreign.ValueLayout.ADDRESS;
import static java.lang.foreign.ValueLayout.JAVA_BYTE;
import static java.lang.foreign.ValueLayout.JAVA_DOUBLE;
import static java.lang.foreign.ValueLayout.JAVA_INT;
import static java.lang.foreign.ValueLayout.JAVA_LONG;

import java.lang.foreign.MemoryLayout;
import java.lang.foreign.PaddingLayout;
import java.lang.foreign.StructLayout;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.function.Function;
import org.jspecify.annotations.Nullable;
import sa.ingenious.pdf.TableStrategy;

/**
 * Everything this jar mirrors from the native library by hand: struct layouts, option bits, table
 * strategy codes and status codes. {@link #verify} checks all of it against {@code
 * bolivar_describe} when the library loads.
 */
// Ordinals are the ABI's bit positions and codes by design; verify() checks them at load.
@SuppressWarnings("EnumOrdinal")
public final class Contract {
  private Contract() {}

  /** {@code options::Options}. */
  public static final StructLayout OPTIONS =
      structLayout(
          JAVA_INT.withName("struct_size"),
          JAVA_INT.withName("present"),
          JAVA_BYTE.withName("caching"),
          JAVA_BYTE.withName("bidi"),
          JAVA_BYTE.withName("detect_vertical"),
          JAVA_BYTE.withName("all_texts"),
          paddingLayout(4),
          JAVA_LONG.withName("max_pages"),
          JAVA_DOUBLE.withName("line_overlap"),
          JAVA_DOUBLE.withName("char_margin"),
          JAVA_DOUBLE.withName("line_margin"),
          JAVA_DOUBLE.withName("word_margin"),
          JAVA_DOUBLE.withName("boxes_flow"),
          ADDRESS.withName("password"),
          JAVA_LONG.withName("password_len"),
          ADDRESS.withName("pages"),
          JAVA_LONG.withName("pages_len"));

  /** {@code options::TableOptions}. */
  public static final StructLayout TABLE_OPTIONS =
      structLayout(
          JAVA_INT.withName("struct_size"),
          JAVA_INT.withName("present"),
          JAVA_INT.withName("vertical_strategy"),
          JAVA_INT.withName("horizontal_strategy"),
          JAVA_DOUBLE.withName("snap"),
          JAVA_DOUBLE.withName("snap_x"),
          JAVA_DOUBLE.withName("snap_y"),
          JAVA_DOUBLE.withName("join"),
          JAVA_DOUBLE.withName("join_x"),
          JAVA_DOUBLE.withName("join_y"),
          JAVA_DOUBLE.withName("intersection"),
          JAVA_DOUBLE.withName("intersection_x"),
          JAVA_DOUBLE.withName("intersection_y"),
          sequenceLayout(4, JAVA_DOUBLE).withName("crop"),
          sequenceLayout(4, JAVA_DOUBLE).withName("first_page_crop"),
          ADDRESS.withName("explicit_vertical"),
          JAVA_LONG.withName("explicit_vertical_len"),
          ADDRESS.withName("explicit_horizontal"),
          JAVA_LONG.withName("explicit_horizontal_len"));

  /** {@code options::CursorOptions}. */
  public static final StructLayout CURSOR_OPTIONS =
      structLayout(
          JAVA_INT.withName("struct_size"),
          JAVA_INT.withName("present"),
          JAVA_LONG.withName("skip"),
          JAVA_LONG.withName("max_pages"),
          JAVA_LONG.withName("read_ahead"),
          ADDRESS.withName("pages"),
          JAVA_LONG.withName("pages_len"));

  /** {@code abi::ErrorOut}: a code, then the field and message as UTF-16 pointer and length. */
  public static final StructLayout ERROR_OUT =
      structLayout(
          JAVA_INT.withName("code"),
          paddingLayout(4),
          ADDRESS.withName("field"),
          JAVA_LONG.withName("field_len"),
          ADDRESS.withName("message"),
          JAVA_LONG.withName("message_len"));

  /** {@code batch::BatchHeader}. */
  public static final StructLayout BATCH_HEADER =
      structLayout(
          JAVA_INT.withName("magic"),
          JAVA_INT.withName("abi_version"),
          JAVA_INT.withName("kind"),
          JAVA_INT.withName("ncols"),
          ADDRESS.withName("cols"));

  /** {@code batch::ColumnDesc}. */
  public static final StructLayout COLUMN_DESC =
      structLayout(
          ADDRESS.withName("ptr"),
          JAVA_LONG.withName("len"),
          JAVA_INT.withName("elem"),
          paddingLayout(4));

  /** The byte offset of a named field. */
  public static long offset(StructLayout layout, String name) {
    return layout.byteOffset(MemoryLayout.PathElement.groupElement(name));
  }

  /** Presence bits of {@code Options.present}, as {@code options::opt}. */
  public enum OptionBit {
    PASSWORD,
    PAGES,
    MAX_PAGES,
    CACHING,
    BIDI,
    LINE_OVERLAP,
    CHAR_MARGIN,
    LINE_MARGIN,
    WORD_MARGIN,
    BOXES_FLOW,
    BOXES_FLOW_NONE,
    DETECT_VERTICAL,
    ALL_TEXTS;

    /** This bit's mask. */
    public int mask() {
      return 1 << ordinal();
    }
  }

  /** Presence bits of {@code TableOptions.present}, as {@code options::topt}. */
  public enum TableOptionBit {
    VERTICAL_STRATEGY,
    HORIZONTAL_STRATEGY,
    SNAP,
    SNAP_X,
    SNAP_Y,
    JOIN,
    JOIN_X,
    JOIN_Y,
    INTERSECTION,
    INTERSECTION_X,
    INTERSECTION_Y,
    EXPLICIT_VERTICAL,
    EXPLICIT_HORIZONTAL,
    CROP,
    FIRST_PAGE_CROP;

    /** This bit's mask. */
    public int mask() {
      return 1 << ordinal();
    }
  }

  /** Presence bits of {@code CursorOptions.present}, as {@code options::copt}. */
  public enum CursorOptionBit {
    PAGES,
    SKIP,
    MAX_PAGES,
    READ_AHEAD;

    /** This bit's mask. */
    public int mask() {
      return 1 << ordinal();
    }
  }

  /** Native status codes, as {@code abi::Code}; 0 is success. */
  public enum Status {
    INVALID_PATH,
    INVALID_ARGUMENT,
    IO_NOT_FOUND,
    IO_PERMISSION_DENIED,
    IO_ERROR,
    SYNTAX_ERROR,
    ENCRYPTION_ERROR,
    MALFORMED_PDF,
    DECODE_ERROR,
    CANCELLED,
    RUNTIME_ERROR,
    PANICKED,
    UNSUPPORTED;

    /** This status's native code. */
    public int code() {
      return ordinal() + 1;
    }

    private static final Status[] BY_CODE = values();

    /** The status with this native code, or null for success or an unknown code. */
    public static @Nullable Status of(int code) {
      return code >= 1 && code <= BY_CODE.length ? BY_CODE[code - 1] : null;
    }
  }

  /** The description {@code bolivar_describe} gives of a struct layout. */
  static String describe(StructLayout layout) {
    List<String> lines = new ArrayList<>();
    lines.add("size:" + layout.byteSize());
    long offset = 0;
    for (MemoryLayout member : layout.memberLayouts()) {
      if (!(member instanceof PaddingLayout)) {
        String name = member.name().orElseThrow();
        lines.add("field:" + name + ":" + offset + ":" + member.byteSize());
      }
      offset += member.byteSize();
    }
    return String.join("\n", lines);
  }

  private static <E extends Enum<E>> String values(E[] values, Function<E, Integer> value) {
    List<String> lines = new ArrayList<>(values.length);
    for (E entry : values) {
      lines.add("value:" + entry.name() + ":" + value.apply(entry));
    }
    return String.join("\n", lines);
  }

  /** What this jar expects {@code bolivar_describe} to give, by name. */
  static Map<String, String> expected() {
    return Map.ofEntries(
        Map.entry("Options", describe(OPTIONS)),
        Map.entry("TableOptions", describe(TABLE_OPTIONS)),
        Map.entry("CursorOptions", describe(CURSOR_OPTIONS)),
        Map.entry("ErrorOut", describe(ERROR_OUT)),
        Map.entry("BatchHeader", describe(BATCH_HEADER)),
        Map.entry("ColumnDesc", describe(COLUMN_DESC)),
        Map.entry("OptionBits", values(OptionBit.values(), OptionBit::mask)),
        Map.entry("TableOptionBits", values(TableOptionBit.values(), TableOptionBit::mask)),
        Map.entry("CursorOptionBits", values(CursorOptionBit.values(), CursorOptionBit::mask)),
        Map.entry("TableStrategy", values(TableStrategy.values(), Enum::ordinal)),
        Map.entry("Status", values(Status.values(), Status::code)));
  }

  /**
   * Check every description against the native one.
   *
   * @throws IllegalStateException naming the first line that differs
   */
  public static void verify(Function<String, String> nativeDescription) {
    expected().forEach((name, text) -> require(name, text, nativeDescription.apply(name)));
  }

  /** Throw unless {@code actual} equals {@code expected}, naming the first line that differs. */
  static void require(String what, String expected, String actual) {
    if (expected.equals(actual)) {
      return;
    }
    String[] want = expected.split("\n", -1);
    String[] got = actual.split("\n", -1);
    int line = 0;
    while (line < want.length && line < got.length && want[line].equals(got[line])) {
      line++;
    }
    throw new IllegalStateException(
        "The Bolivar jar and native library disagree on "
            + what
            + " at line "
            + (line + 1)
            + ": jar has "
            + (line < want.length ? "'" + want[line] + "'" : "nothing")
            + ", native has "
            + (line < got.length ? "'" + got[line] + "'" : "nothing")
            + ". Use a jar and native library from the same build.");
  }
}
