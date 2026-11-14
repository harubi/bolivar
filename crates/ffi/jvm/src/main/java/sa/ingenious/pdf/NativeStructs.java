package sa.ingenious.pdf;

import static java.lang.foreign.ValueLayout.ADDRESS;
import static java.lang.foreign.ValueLayout.JAVA_BYTE;
import static java.lang.foreign.ValueLayout.JAVA_DOUBLE;
import static java.lang.foreign.ValueLayout.JAVA_INT;
import static java.lang.foreign.ValueLayout.JAVA_LONG;
import static sa.ingenious.pdf.internal.Contract.OPTIONS;
import static sa.ingenious.pdf.internal.Contract.TABLE_OPTIONS;
import static sa.ingenious.pdf.internal.Contract.offset;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.nio.charset.StandardCharsets;
import java.util.List;
import org.jspecify.annotations.Nullable;
import sa.ingenious.pdf.internal.Contract.OptionBit;
import sa.ingenious.pdf.internal.Contract.TableOptionBit;

/**
 * Writes options into the native {@code #[repr(C)]} structs of {@code crates/ffi/src/options.rs}.
 * Only types and presence are settled here; the native side checks every value.
 */
// TableStrategy ordinals are the native strategy codes; Contract.verify checks them at load.
@SuppressWarnings("EnumOrdinal")
final class NativeStructs {
  private NativeStructs() {}

  private static final long STRUCT_SIZE = offset(OPTIONS, "struct_size");
  private static final long PRESENT = offset(OPTIONS, "present");
  private static final long CACHING = offset(OPTIONS, "caching");
  private static final long BIDI = offset(OPTIONS, "bidi");
  private static final long DETECT_VERTICAL = offset(OPTIONS, "detect_vertical");
  private static final long ALL_TEXTS = offset(OPTIONS, "all_texts");
  private static final long MAX_PAGES = offset(OPTIONS, "max_pages");
  private static final long LINE_OVERLAP = offset(OPTIONS, "line_overlap");
  private static final long CHAR_MARGIN = offset(OPTIONS, "char_margin");
  private static final long LINE_MARGIN = offset(OPTIONS, "line_margin");
  private static final long WORD_MARGIN = offset(OPTIONS, "word_margin");
  private static final long BOXES_FLOW = offset(OPTIONS, "boxes_flow");
  private static final long PASSWORD = offset(OPTIONS, "password");
  private static final long PASSWORD_LEN = offset(OPTIONS, "password_len");
  private static final long PAGES = offset(OPTIONS, "pages");
  private static final long PAGES_LEN = offset(OPTIONS, "pages_len");

  private static final long T_STRUCT_SIZE = offset(TABLE_OPTIONS, "struct_size");
  private static final long T_PRESENT = offset(TABLE_OPTIONS, "present");
  private static final long T_VERTICAL = offset(TABLE_OPTIONS, "vertical_strategy");
  private static final long T_HORIZONTAL = offset(TABLE_OPTIONS, "horizontal_strategy");
  private static final long T_SNAP = offset(TABLE_OPTIONS, "snap");
  private static final long T_CROP = offset(TABLE_OPTIONS, "crop");
  private static final long T_FIRST_PAGE_CROP = offset(TABLE_OPTIONS, "first_page_crop");
  private static final long T_MAX_PAGES = offset(TABLE_OPTIONS, "max_pages");
  private static final long T_VERTICAL_LINES = offset(TABLE_OPTIONS, "explicit_vertical");
  private static final long T_VERTICAL_LINES_LEN = offset(TABLE_OPTIONS, "explicit_vertical_len");
  private static final long T_HORIZONTAL_LINES = offset(TABLE_OPTIONS, "explicit_horizontal");
  private static final long T_HORIZONTAL_LINES_LEN =
      offset(TABLE_OPTIONS, "explicit_horizontal_len");

  private static byte flag(boolean value) {
    return (byte) (value ? 1 : 0);
  }

  /** The native options, or {@code NULL} when every field is the default. */
  static MemorySegment options(@Nullable Options options, Arena arena) {
    if (options == null || options.equals(Options.DEFAULT)) {
      return MemorySegment.NULL;
    }
    var s = arena.allocate(OPTIONS);
    int present = 0;
    String password = options.password();
    if (password != null) {
      byte[] bytes = password.getBytes(StandardCharsets.UTF_8);
      s.set(ADDRESS, PASSWORD, arena.allocateFrom(JAVA_BYTE, bytes));
      s.set(JAVA_LONG, PASSWORD_LEN, bytes.length);
      present |= OptionBit.PASSWORD.mask();
    }
    List<Integer> pages = options.pages();
    if (pages != null) {
      long[] values = new long[pages.size()];
      for (int i = 0; i < values.length; i++) {
        values[i] = pages.get(i);
      }
      if (values.length > 0) {
        s.set(ADDRESS, PAGES, arena.allocateFrom(JAVA_LONG, values));
      }
      s.set(JAVA_LONG, PAGES_LEN, values.length);
      present |= OptionBit.PAGES.mask();
    }
    Integer maxPages = options.maxPages();
    if (maxPages != null) {
      s.set(JAVA_LONG, MAX_PAGES, maxPages);
      present |= OptionBit.MAX_PAGES.mask();
    }
    present |= setFlag(s, CACHING, options.caching(), OptionBit.CACHING);
    present |= setFlag(s, BIDI, options.bidi(), OptionBit.BIDI);
    LayoutOptions layout = options.layout();
    if (layout != null) {
      present |= setDouble(s, LINE_OVERLAP, layout.lineOverlap(), OptionBit.LINE_OVERLAP);
      present |= setDouble(s, CHAR_MARGIN, layout.charMargin(), OptionBit.CHAR_MARGIN);
      present |= setDouble(s, LINE_MARGIN, layout.lineMargin(), OptionBit.LINE_MARGIN);
      present |= setDouble(s, WORD_MARGIN, layout.wordMargin(), OptionBit.WORD_MARGIN);
      present |= setDouble(s, BOXES_FLOW, layout.boxesFlow(), OptionBit.BOXES_FLOW);
      if (layout.boxesFlowDisabled()) {
        present |= OptionBit.BOXES_FLOW_NONE.mask();
      }
      present |= setFlag(s, DETECT_VERTICAL, layout.detectVertical(), OptionBit.DETECT_VERTICAL);
      present |= setFlag(s, ALL_TEXTS, layout.allTexts(), OptionBit.ALL_TEXTS);
    }
    s.set(JAVA_INT, STRUCT_SIZE, (int) OPTIONS.byteSize());
    s.set(JAVA_INT, PRESENT, present);
    return s;
  }

  private static int setFlag(MemorySegment s, long offset, @Nullable Boolean value, OptionBit bit) {
    if (value == null) {
      return 0;
    }
    s.set(JAVA_BYTE, offset, flag(value));
    return bit.mask();
  }

  private static int setDouble(
      MemorySegment s, long offset, @Nullable Double value, OptionBit bit) {
    if (value == null) {
      return 0;
    }
    s.set(JAVA_DOUBLE, offset, value);
    return bit.mask();
  }

  /** The native table options, or {@code NULL} for the defaults. */
  static MemorySegment tableOptions(@Nullable TableOptions options, Arena arena) {
    if (options == null || options.equals(TableOptions.DEFAULT)) {
      return MemorySegment.NULL;
    }
    var s = arena.allocate(TABLE_OPTIONS);
    int present = 0;
    TableStrategy vertical = options.verticalStrategy();
    if (vertical != null) {
      s.set(JAVA_INT, T_VERTICAL, vertical.ordinal());
      present |= TableOptionBit.VERTICAL_STRATEGY.mask();
    }
    TableStrategy horizontal = options.horizontalStrategy();
    if (horizontal != null) {
      s.set(JAVA_INT, T_HORIZONTAL, horizontal.ordinal());
      present |= TableOptionBit.HORIZONTAL_STRATEGY.mask();
    }
    // The nine tolerances sit in order from `snap`, as their bits do from SNAP.
    @Nullable Double[] tolerances = {
      options.snapTolerance(),
      options.snapXTolerance(),
      options.snapYTolerance(),
      options.joinTolerance(),
      options.joinXTolerance(),
      options.joinYTolerance(),
      options.intersectionTolerance(),
      options.intersectionXTolerance(),
      options.intersectionYTolerance(),
    };
    for (int i = 0; i < tolerances.length; i++) {
      Double tolerance = tolerances[i];
      if (tolerance != null) {
        s.set(JAVA_DOUBLE, T_SNAP + 8L * i, tolerance);
        present |= TableOptionBit.SNAP.mask() << i;
      }
    }
    present |=
        lines(
            s,
            arena,
            T_VERTICAL_LINES,
            T_VERTICAL_LINES_LEN,
            options.explicitVerticalLines(),
            TableOptionBit.EXPLICIT_VERTICAL);
    present |=
        lines(
            s,
            arena,
            T_HORIZONTAL_LINES,
            T_HORIZONTAL_LINES_LEN,
            options.explicitHorizontalLines(),
            TableOptionBit.EXPLICIT_HORIZONTAL);
    present |= crop(s, T_CROP, options.crop(), TableOptionBit.CROP);
    present |= crop(s, T_FIRST_PAGE_CROP, options.firstPageCrop(), TableOptionBit.FIRST_PAGE_CROP);
    Integer maxPages = options.maxPages();
    if (maxPages != null) {
      s.set(JAVA_LONG, T_MAX_PAGES, maxPages);
      present |= TableOptionBit.MAX_PAGES.mask();
    }
    s.set(JAVA_INT, T_STRUCT_SIZE, (int) TABLE_OPTIONS.byteSize());
    s.set(JAVA_INT, T_PRESENT, present);
    return s;
  }

  private static int lines(
      MemorySegment s,
      Arena arena,
      long pointer,
      long length,
      @Nullable List<Double> lines,
      TableOptionBit bit) {
    if (lines == null) {
      return 0;
    }
    double[] values = new double[lines.size()];
    for (int i = 0; i < values.length; i++) {
      values[i] = lines.get(i);
    }
    if (values.length > 0) {
      s.set(ADDRESS, pointer, arena.allocateFrom(JAVA_DOUBLE, values));
    }
    s.set(JAVA_LONG, length, values.length);
    return bit.mask();
  }

  private static int crop(
      MemorySegment s, long base, @Nullable BoundingBox box, TableOptionBit bit) {
    if (box == null) {
      return 0;
    }
    s.set(JAVA_DOUBLE, base, box.x0());
    s.set(JAVA_DOUBLE, base + 8, box.y0());
    s.set(JAVA_DOUBLE, base + 16, box.x1());
    s.set(JAVA_DOUBLE, base + 24, box.y1());
    return bit.mask();
  }
}
