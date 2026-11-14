package sa.ingenious.pdf;

import static java.lang.foreign.ValueLayout.ADDRESS;
import static java.lang.foreign.ValueLayout.JAVA_BYTE;
import static java.lang.foreign.ValueLayout.JAVA_DOUBLE;
import static java.lang.foreign.ValueLayout.JAVA_INT;
import static java.lang.foreign.ValueLayout.JAVA_LONG;
import static sa.ingenious.pdf.internal.Contract.CURSOR_OPTIONS;
import static sa.ingenious.pdf.internal.Contract.OPTIONS;
import static sa.ingenious.pdf.internal.Contract.TABLE_OPTIONS;
import static sa.ingenious.pdf.internal.Contract.offset;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.nio.charset.StandardCharsets;
import java.util.List;
import org.jspecify.annotations.Nullable;
import sa.ingenious.pdf.internal.Contract.CursorOptionBit;
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
  private static final long T_CROP = offset(TABLE_OPTIONS, "crop");
  private static final long T_FIRST_PAGE_CROP = offset(TABLE_OPTIONS, "first_page_crop");
  private static final long T_MAX_PAGES = offset(TABLE_OPTIONS, "max_pages");
  private static final long T_VERTICAL_LINES = offset(TABLE_OPTIONS, "explicit_vertical");
  private static final long T_VERTICAL_LINES_LEN = offset(TABLE_OPTIONS, "explicit_vertical_len");
  private static final long T_HORIZONTAL_LINES = offset(TABLE_OPTIONS, "explicit_horizontal");
  private static final long T_HORIZONTAL_LINES_LEN =
      offset(TABLE_OPTIONS, "explicit_horizontal_len");

  private static final long C_STRUCT_SIZE = offset(CURSOR_OPTIONS, "struct_size");
  private static final long C_PRESENT = offset(CURSOR_OPTIONS, "present");
  private static final long C_SKIP = offset(CURSOR_OPTIONS, "skip");
  private static final long C_MAX_PAGES = offset(CURSOR_OPTIONS, "max_pages");
  private static final long C_READ_AHEAD = offset(CURSOR_OPTIONS, "read_ahead");
  private static final long C_PAGES = offset(CURSOR_OPTIONS, "pages");
  private static final long C_PAGES_LEN = offset(CURSOR_OPTIONS, "pages_len");

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
    present |=
        setArray(s, arena, PAGES, PAGES_LEN, options.pages(), JAVA_LONG, OptionBit.PAGES.mask());
    present |= setLong(s, MAX_PAGES, options.maxPages(), OptionBit.MAX_PAGES.mask());
    present |= setFlag(s, CACHING, options.caching(), OptionBit.CACHING.mask());
    present |= setFlag(s, BIDI, options.bidi(), OptionBit.BIDI.mask());
    LayoutOptions layout = options.layout();
    if (layout != null) {
      present |= setDouble(s, LINE_OVERLAP, layout.lineOverlap(), OptionBit.LINE_OVERLAP.mask());
      present |= setDouble(s, CHAR_MARGIN, layout.charMargin(), OptionBit.CHAR_MARGIN.mask());
      present |= setDouble(s, LINE_MARGIN, layout.lineMargin(), OptionBit.LINE_MARGIN.mask());
      present |= setDouble(s, WORD_MARGIN, layout.wordMargin(), OptionBit.WORD_MARGIN.mask());
      present |= setDouble(s, BOXES_FLOW, layout.boxesFlow(), OptionBit.BOXES_FLOW.mask());
      if (layout.boxesFlowDisabled()) {
        present |= OptionBit.BOXES_FLOW_NONE.mask();
      }
      present |=
          setFlag(s, DETECT_VERTICAL, layout.detectVertical(), OptionBit.DETECT_VERTICAL.mask());
      present |= setFlag(s, ALL_TEXTS, layout.allTexts(), OptionBit.ALL_TEXTS.mask());
    }
    s.set(JAVA_INT, STRUCT_SIZE, (int) OPTIONS.byteSize());
    s.set(JAVA_INT, PRESENT, present);
    return s;
  }

  // Each setter writes a present value and returns its presence bit, or writes nothing and
  // returns 0.

  private static int setFlag(MemorySegment s, long offset, @Nullable Boolean value, int bit) {
    if (value == null) {
      return 0;
    }
    s.set(JAVA_BYTE, offset, (byte) (value ? 1 : 0));
    return bit;
  }

  private static int setDouble(MemorySegment s, long offset, @Nullable Double value, int bit) {
    if (value == null) {
      return 0;
    }
    s.set(JAVA_DOUBLE, offset, value);
    return bit;
  }

  private static int setLong(MemorySegment s, long offset, @Nullable Integer value, int bit) {
    if (value == null) {
      return 0;
    }
    s.set(JAVA_LONG, offset, value);
    return bit;
  }

  /**
   * A pointer and length pair. An empty array leaves the pointer {@code NULL}, as the native side
   * expects; {@code layout} is {@code JAVA_LONG} or {@code JAVA_DOUBLE}.
   */
  private static int setArray(
      MemorySegment s,
      Arena arena,
      long pointer,
      long length,
      @Nullable List<? extends Number> values,
      ValueLayout layout,
      int bit) {
    if (values == null) {
      return 0;
    }
    if (!values.isEmpty()) {
      MemorySegment array = arena.allocate(layout, values.size());
      for (int i = 0; i < values.size(); i++) {
        if (layout instanceof ValueLayout.OfLong) {
          array.setAtIndex(JAVA_LONG, i, values.get(i).longValue());
        } else {
          array.setAtIndex(JAVA_DOUBLE, i, values.get(i).doubleValue());
        }
      }
      s.set(ADDRESS, pointer, array);
    }
    s.set(JAVA_LONG, length, values.size());
    return bit;
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
    present |= tolerance(s, "snap", options.snapTolerance(), TableOptionBit.SNAP);
    present |= tolerance(s, "snap_x", options.snapXTolerance(), TableOptionBit.SNAP_X);
    present |= tolerance(s, "snap_y", options.snapYTolerance(), TableOptionBit.SNAP_Y);
    present |= tolerance(s, "join", options.joinTolerance(), TableOptionBit.JOIN);
    present |= tolerance(s, "join_x", options.joinXTolerance(), TableOptionBit.JOIN_X);
    present |= tolerance(s, "join_y", options.joinYTolerance(), TableOptionBit.JOIN_Y);
    present |=
        tolerance(s, "intersection", options.intersectionTolerance(), TableOptionBit.INTERSECTION);
    present |=
        tolerance(
            s, "intersection_x", options.intersectionXTolerance(), TableOptionBit.INTERSECTION_X);
    present |=
        tolerance(
            s, "intersection_y", options.intersectionYTolerance(), TableOptionBit.INTERSECTION_Y);
    present |=
        setArray(
            s,
            arena,
            T_VERTICAL_LINES,
            T_VERTICAL_LINES_LEN,
            options.explicitVerticalLines(),
            JAVA_DOUBLE,
            TableOptionBit.EXPLICIT_VERTICAL.mask());
    present |=
        setArray(
            s,
            arena,
            T_HORIZONTAL_LINES,
            T_HORIZONTAL_LINES_LEN,
            options.explicitHorizontalLines(),
            JAVA_DOUBLE,
            TableOptionBit.EXPLICIT_HORIZONTAL.mask());
    present |= crop(s, T_CROP, options.crop(), TableOptionBit.CROP);
    present |= crop(s, T_FIRST_PAGE_CROP, options.firstPageCrop(), TableOptionBit.FIRST_PAGE_CROP);
    present |= setLong(s, T_MAX_PAGES, options.maxPages(), TableOptionBit.MAX_PAGES.mask());
    s.set(JAVA_INT, T_STRUCT_SIZE, (int) TABLE_OPTIONS.byteSize());
    s.set(JAVA_INT, T_PRESENT, present);
    return s;
  }

  /**
   * The native cursor options, or {@code NULL} for the defaults, so a plain cursor allocates none.
   */
  static MemorySegment cursorOptions(@Nullable CursorOptions options, Arena arena) {
    if (options == null || options.equals(CursorOptions.DEFAULT)) {
      return MemorySegment.NULL;
    }
    var s = arena.allocate(CURSOR_OPTIONS);
    int present =
        setArray(
            s,
            arena,
            C_PAGES,
            C_PAGES_LEN,
            options.pages(),
            JAVA_LONG,
            CursorOptionBit.PAGES.mask());
    present |= setLong(s, C_SKIP, options.skip(), CursorOptionBit.SKIP.mask());
    present |= setLong(s, C_MAX_PAGES, options.maxPages(), CursorOptionBit.MAX_PAGES.mask());
    present |= setLong(s, C_READ_AHEAD, options.readAhead(), CursorOptionBit.READ_AHEAD.mask());
    s.set(JAVA_INT, C_STRUCT_SIZE, (int) CURSOR_OPTIONS.byteSize());
    s.set(JAVA_INT, C_PRESENT, present);
    return s;
  }

  /** A tolerance, by its field's name in the native struct. */
  private static int tolerance(
      MemorySegment s, String field, @Nullable Double value, TableOptionBit bit) {
    return setDouble(s, offset(TABLE_OPTIONS, field), value, bit.mask());
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
