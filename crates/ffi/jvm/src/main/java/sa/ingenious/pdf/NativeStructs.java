package sa.ingenious.pdf;

import static java.lang.foreign.ValueLayout.ADDRESS;
import static java.lang.foreign.ValueLayout.JAVA_BYTE;
import static java.lang.foreign.ValueLayout.JAVA_DOUBLE;
import static java.lang.foreign.ValueLayout.JAVA_INT;
import static java.lang.foreign.ValueLayout.JAVA_LONG;

import java.lang.foreign.Arena;
import java.lang.foreign.MemoryLayout;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.StructLayout;
import java.nio.charset.StandardCharsets;
import java.util.List;

/** Writes options into the native {@code #[repr(C)]} structs of {@code crates/ffi/src/options.rs}. */
final class NativeStructs {
  private NativeStructs() {}

  static final StructLayout OPTIONS =
      MemoryLayout.structLayout(
          JAVA_INT.withName("struct_size"),
          JAVA_INT.withName("present"),
          JAVA_BYTE.withName("caching"),
          JAVA_BYTE.withName("bidi"),
          JAVA_BYTE.withName("detect_vertical"),
          JAVA_BYTE.withName("all_texts"),
          JAVA_INT.withName("pad"),
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

  static final StructLayout TABLE_OPTIONS =
      MemoryLayout.structLayout(
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
          MemoryLayout.sequenceLayout(4, JAVA_DOUBLE).withName("crop"),
          MemoryLayout.sequenceLayout(4, JAVA_DOUBLE).withName("first_page_crop"),
          JAVA_LONG.withName("max_pages"),
          ADDRESS.withName("explicit_vertical"),
          JAVA_LONG.withName("explicit_vertical_len"),
          ADDRESS.withName("explicit_horizontal"),
          JAVA_LONG.withName("explicit_horizontal_len"));

  // Presence bits, as `options::opt` and `options::topt`.
  private static final int PASSWORD = 1;
  private static final int PAGES = 1 << 1;
  private static final int MAX_PAGES = 1 << 2;
  private static final int CACHING = 1 << 3;
  private static final int BIDI = 1 << 4;
  private static final int LINE_OVERLAP = 1 << 5;
  private static final int CHAR_MARGIN = 1 << 6;
  private static final int LINE_MARGIN = 1 << 7;
  private static final int WORD_MARGIN = 1 << 8;
  private static final int BOXES_FLOW = 1 << 9;
  private static final int BOXES_FLOW_NONE = 1 << 10;
  private static final int DETECT_VERTICAL = 1 << 11;
  private static final int ALL_TEXTS = 1 << 12;

  private static long offset(StructLayout layout, String name) {
    return layout.byteOffset(MemoryLayout.PathElement.groupElement(name));
  }

  private static byte flag(boolean value) {
    return (byte) (value ? 1 : 0);
  }

  /** The native options, or {@code NULL} when every field is the default. */
  static MemorySegment options(Options options, Arena arena) {
    if (options == null || options.equals(Options.DEFAULT)) {
      return MemorySegment.NULL;
    }
    var s = arena.allocate(OPTIONS);
    int present = 0;
    if (options.password() != null) {
      byte[] bytes = options.password().getBytes(StandardCharsets.UTF_8);
      s.set(ADDRESS, offset(OPTIONS, "password"), arena.allocateFrom(JAVA_BYTE, bytes));
      s.set(JAVA_LONG, offset(OPTIONS, "password_len"), bytes.length);
      present |= PASSWORD;
    }
    if (options.pages() != null) {
      long[] pages = options.pages().stream().mapToLong(Integer::longValue).toArray();
      if (pages.length > 0) {
        s.set(ADDRESS, offset(OPTIONS, "pages"), arena.allocateFrom(JAVA_LONG, pages));
      }
      s.set(JAVA_LONG, offset(OPTIONS, "pages_len"), pages.length);
      present |= PAGES;
    }
    if (options.maxPages() != null) {
      s.set(JAVA_LONG, offset(OPTIONS, "max_pages"), options.maxPages());
      present |= MAX_PAGES;
    }
    if (options.caching() != null) {
      s.set(JAVA_BYTE, offset(OPTIONS, "caching"), flag(options.caching()));
      present |= CACHING;
    }
    if (options.bidi() != null) {
      s.set(JAVA_BYTE, offset(OPTIONS, "bidi"), flag(options.bidi()));
      present |= BIDI;
    }
    LayoutOptions layout = options.layout();
    if (layout != null) {
      present |= setDouble(s, "line_overlap", layout.lineOverlap(), LINE_OVERLAP);
      present |= setDouble(s, "char_margin", layout.charMargin(), CHAR_MARGIN);
      present |= setDouble(s, "line_margin", layout.lineMargin(), LINE_MARGIN);
      present |= setDouble(s, "word_margin", layout.wordMargin(), WORD_MARGIN);
      present |= setDouble(s, "boxes_flow", layout.boxesFlow(), BOXES_FLOW);
      if (layout.boxesFlowDisabled()) {
        present |= BOXES_FLOW_NONE;
      }
      if (layout.detectVertical() != null) {
        s.set(JAVA_BYTE, offset(OPTIONS, "detect_vertical"), flag(layout.detectVertical()));
        present |= DETECT_VERTICAL;
      }
      if (layout.allTexts() != null) {
        s.set(JAVA_BYTE, offset(OPTIONS, "all_texts"), flag(layout.allTexts()));
        present |= ALL_TEXTS;
      }
    }
    s.set(JAVA_INT, offset(OPTIONS, "struct_size"), (int) OPTIONS.byteSize());
    s.set(JAVA_INT, offset(OPTIONS, "present"), present);
    return s;
  }

  private static int setDouble(MemorySegment s, String name, Double value, int bit) {
    if (value == null) {
      return 0;
    }
    s.set(JAVA_DOUBLE, offset(OPTIONS, name), value);
    return bit;
  }

  /** The native table options, or {@code NULL} for the defaults. */
  static MemorySegment tableOptions(TableOptions options, Arena arena) {
    if (options == null || options.equals(TableOptions.DEFAULT)) {
      return MemorySegment.NULL;
    }
    var s = arena.allocate(TABLE_OPTIONS);
    int present = 0;
    if (options.verticalStrategy() != null) {
      s.set(JAVA_INT, offset(TABLE_OPTIONS, "vertical_strategy"), options.verticalStrategy().ordinal());
      present |= 1;
    }
    if (options.horizontalStrategy() != null) {
      s.set(
          JAVA_INT,
          offset(TABLE_OPTIONS, "horizontal_strategy"),
          options.horizontalStrategy().ordinal());
      present |= 1 << 1;
    }
    Double[] tolerances = {
      options.snapTolerance(), options.snapXTolerance(), options.snapYTolerance(),
      options.joinTolerance(), options.joinXTolerance(), options.joinYTolerance(),
      options.intersectionTolerance(), options.intersectionXTolerance(),
      options.intersectionYTolerance(),
    };
    long first = offset(TABLE_OPTIONS, "snap");
    for (int i = 0; i < tolerances.length; i++) {
      if (tolerances[i] != null) {
        s.set(JAVA_DOUBLE, first + 8L * i, tolerances[i]);
        present |= 1 << (2 + i);
      }
    }
    present |= lines(s, arena, "explicit_vertical", options.explicitVerticalLines(), 1 << 11);
    present |= lines(s, arena, "explicit_horizontal", options.explicitHorizontalLines(), 1 << 12);
    present |= crop(s, "crop", options.crop(), 1 << 13);
    present |= crop(s, "first_page_crop", options.firstPageCrop(), 1 << 14);
    if (options.maxPages() != null) {
      s.set(JAVA_LONG, offset(TABLE_OPTIONS, "max_pages"), options.maxPages());
      present |= 1 << 15;
    }
    s.set(JAVA_INT, offset(TABLE_OPTIONS, "struct_size"), (int) TABLE_OPTIONS.byteSize());
    s.set(JAVA_INT, offset(TABLE_OPTIONS, "present"), present);
    return s;
  }

  private static int lines(
      MemorySegment s, Arena arena, String name, List<Double> lines, int bit) {
    if (lines == null) {
      return 0;
    }
    double[] values = lines.stream().mapToDouble(Double::doubleValue).toArray();
    if (values.length > 0) {
      s.set(ADDRESS, offset(TABLE_OPTIONS, name), arena.allocateFrom(JAVA_DOUBLE, values));
    }
    s.set(JAVA_LONG, offset(TABLE_OPTIONS, name + "_len"), values.length);
    return bit;
  }

  private static int crop(MemorySegment s, String name, BoundingBox box, int bit) {
    if (box == null) {
      return 0;
    }
    long base = offset(TABLE_OPTIONS, name);
    s.set(JAVA_DOUBLE, base, box.x0());
    s.set(JAVA_DOUBLE, base + 8, box.y0());
    s.set(JAVA_DOUBLE, base + 16, box.x1());
    s.set(JAVA_DOUBLE, base + 24, box.y1());
    return bit;
  }
}
