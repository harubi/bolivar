package sa.ingenious.pdf.internal;

import static sa.ingenious.pdf.internal.Schema.Elem.F64;
import static sa.ingenious.pdf.internal.Schema.Elem.I32;
import static sa.ingenious.pdf.internal.Schema.Elem.I64;
import static sa.ingenious.pdf.internal.Schema.Elem.U16;
import static sa.ingenious.pdf.internal.Schema.Elem.U8;

import java.util.ArrayList;
import java.util.List;
import java.util.function.IntFunction;
import org.jspecify.annotations.Nullable;

/**
 * Batch kinds, mirroring {@code crates/ffi/src/batch.rs}: each kind's columns, its named slots in
 * the {@code Ints} and {@code Floats} columns, its named ranges and its flag bits. An entry's index
 * is its enum ordinal.
 *
 * <p>{@link #verify} checks every kind against the native {@code bolivar_schema} text when the
 * library loads, so a mismatch fails at once, with the first line that differs.
 */
// Ordinals are the ABI's indices by design: each enum mirrors a Rust declaration in order, and
// verify() checks the order against the native library.
@SuppressWarnings("EnumOrdinal")
public final class Schema {
  private Schema() {}

  /** Column element types, as {@code batch::Elem}. */
  public enum Elem {
    F64,
    I32,
    I64,
    U8,
    U16;

    private static final Elem[] BY_CODE = values();

    /** The element type with this native code: its ordinal plus one. */
    static Elem of(int code) {
      if (code < 1 || code > BY_CODE.length) {
        throw new IllegalStateException("unknown column element type " + code);
      }
      return BY_CODE[code - 1];
    }
  }

  /** A column of some batch kind; the per-kind {@code Col} enums implement it. */
  public interface Col {
    /** The column's position in its batch. */
    default int index() {
      return ordinal();
    }

    int ordinal();

    String name();

    Elem elem();
  }

  /** The five columns every kind starts with, at these positions. */
  public enum Common implements Col {
    Text(U16),
    Strings(U16),
    StringOff(I32),
    Ints(I64),
    Floats(F64);

    private final Elem elem;

    Common(Elem elem) {
      this.elem = elem;
    }

    @Override
    public Elem elem() {
      return elem;
    }
  }

  /** Where a kind's own columns start: after the common ones. */
  static final int OWN = Common.values().length;

  /** The layout page. Raw pages share its columns at the same positions. */
  public static final class Layout {
    private Layout() {}

    public enum Col implements Schema.Col {
      BoxBbox(F64),
      BoxMode(I32),
      BoxText(I32),
      BoxLines(I32),
      LineBbox(F64),
      LineOrient(I32),
      LineText(I32),
      LineChars(I32),
      CharX0(F64),
      CharY0(F64),
      CharX1(F64),
      CharY1(F64),
      CharSize(F64),
      CharFont(I32),
      CharUpright(U8),
      CharText(I32);

      private final Elem elem;

      Col(Elem elem) {
        this.elem = elem;
      }

      @Override
      public int index() {
        return OWN + ordinal();
      }

      @Override
      public Elem elem() {
        return elem;
      }
    }

    public enum IntSlot {
      PageNumber,
      TextEnd
    }

    public enum FloatSlot {
      Rotate,
      X0,
      Y0,
      X1,
      Y1
    }
  }

  /** The raw page: the layout columns, then raw-only ones. */
  public static final class Raw {
    private Raw() {}

    /**
     * The raw-only columns, which follow {@link Layout.Col}: a raw batch is a layout batch with
     * more columns, so one reader serves the text layer of both.
     */
    public enum Col implements Schema.Col {
      LineRawText(I32),
      CharAdvance(F64),
      CharMatrix(F64),
      CharMcid(I32),
      CharTag(I32),
      CharNcs(I32),
      CharScs(I32),
      CharNcolor(I32),
      CharScolor(I32),
      ColorPool(F64),
      PageBoxes(F64),
      PageBoxPresent(U8),
      TableBboxTopDown(F64),
      TableShape(I32),
      TableCells(I32),
      CellIndex(I32),
      CellBboxTopDown(F64),
      CellText(I32);

      private static final int SHARED = OWN + Layout.Col.values().length;

      private final Elem elem;

      Col(Elem elem) {
        this.elem = elem;
      }

      @Override
      public int index() {
        return SHARED + ordinal();
      }

      @Override
      public Elem elem() {
        return elem;
      }
    }

    public enum IntSlot {
      PageIndex,
      ObjectId,
      Rotation,
      LabelStart,
      LabelEnd,
      TextEnd
    }

    public enum FloatSlot {
      UserUnit,
      X0,
      Y0,
      X1,
      Y1
    }
  }

  /** A page's text with and without bidi, and its distinct glyphs. */
  public static final class PageText {
    private PageText() {}

    public enum Col implements Schema.Col {
      BoxBbox(F64),
      BoxText(I32),
      BoxTextWithoutBidi(I32),
      BoxLines(I32),
      LineBbox(F64),
      LineText(I32),
      LineTextWithoutBidi(I32),
      GlyphFont(I32),
      GlyphText(I32),
      GlyphSize(F64);

      private final Elem elem;

      Col(Elem elem) {
        this.elem = elem;
      }

      @Override
      public int index() {
        return OWN + ordinal();
      }

      @Override
      public Elem elem() {
        return elem;
      }
    }

    public enum IntSlot {
      PageNumber
    }

    public enum FloatSlot {
      X0,
      Y0,
      X1,
      Y1
    }
  }

  /** A page's number, box, rotation and text. */
  public static final class Summary {
    private Summary() {}

    public enum IntSlot {
      PageNumber
    }

    public enum FloatSlot {
      Rotate,
      X0,
      Y0,
      X1,
      Y1
    }
  }

  /** A page's tables, with boxes in PDF space. */
  public static final class Tables {
    private Tables() {}

    public enum Col implements Schema.Col {
      TableBbox(F64),
      TableShape(I32),
      TableCells(I32),
      CellIndex(I32),
      CellBbox(F64),
      CellText(I32);

      private final Elem elem;

      Col(Elem elem) {
        this.elem = elem;
      }

      @Override
      public int index() {
        return OWN + ordinal();
      }

      @Override
      public Elem elem() {
        return elem;
      }
    }

    public enum IntSlot {
      PageNumber
    }
  }

  /** A page's raw table rows. */
  public static final class TableRows {
    private TableRows() {}

    public enum Col implements Schema.Col {
      CellText(I32),
      RowOffsets(I32),
      TableOffsets(I32);

      private final Elem elem;

      Col(Elem elem) {
        this.elem = elem;
      }

      @Override
      public int index() {
        return OWN + ordinal();
      }

      @Override
      public Elem elem() {
        return elem;
      }
    }

    public enum IntSlot {
      PageNumber
    }
  }

  /** Document metadata. */
  public static final class Metadata {
    private Metadata() {}

    public enum Col implements Schema.Col {
      Ranges(I32),
      InfoKeys(I32),
      InfoValues(I32);

      private final Elem elem;

      Col(Elem elem) {
        this.elem = elem;
      }

      @Override
      public int index() {
        return OWN + ordinal();
      }

      @Override
      public Elem elem() {
        return elem;
      }
    }

    public enum IntSlot {
      FileSize,
      PageCount,
      Flags
    }

    public enum RangeSlot {
      Title,
      Author,
      Subject,
      Keywords,
      Creator,
      Producer,
      CreationDateRaw,
      CreationDateIso,
      ModificationDateRaw,
      ModificationDateIso,
      VersionHeader,
      VersionCatalog,
      Form,
      Xmp
    }

    /** Bits of the {@code Flags} slot. */
    public enum FlagBit {
      Encrypted,
      Printable,
      Modifiable,
      Extractable,
      Linearized,
      Tagged,
      UserProperties,
      Suspects,
      HasJavascript;

      /** Whether this bit is set in {@code flags}. */
      public boolean in(long flags) {
        return (flags & (1L << ordinal())) != 0;
      }
    }
  }

  /** The whole document's text, as UTF-8. */
  public static final class Text {
    private Text() {}

    public enum Col implements Schema.Col {
      Utf8(U8);

      private final Elem elem;

      Col(Elem elem) {
        this.elem = elem;
      }

      @Override
      public int index() {
        return OWN + ordinal();
      }

      @Override
      public Elem elem() {
        return elem;
      }
    }
  }

  private static final Enum<?>[] NONE = {};

  /** Every batch kind with its native number. */
  // The lists are List.of, so unmodifiable; the checker only trusts Guava's types.
  @SuppressWarnings("ImmutableEnumChecker")
  public enum Kind {
    LAYOUT(
        1,
        columns(Layout.Col.values()),
        Layout.IntSlot.values(),
        Layout.FloatSlot.values(),
        NONE,
        NONE),
    RAW(
        2,
        columns(Layout.Col.values(), Raw.Col.values()),
        Raw.IntSlot.values(),
        Raw.FloatSlot.values(),
        NONE,
        NONE),
    PAGE_TEXT(
        3,
        columns(PageText.Col.values()),
        PageText.IntSlot.values(),
        PageText.FloatSlot.values(),
        NONE,
        NONE),
    SUMMARY(4, columns(), Summary.IntSlot.values(), Summary.FloatSlot.values(), NONE, NONE),
    TABLES(5, columns(Tables.Col.values()), Tables.IntSlot.values(), NONE, NONE, NONE),
    TABLE_ROWS(6, columns(TableRows.Col.values()), TableRows.IntSlot.values(), NONE, NONE, NONE),
    METADATA(
        7,
        columns(Metadata.Col.values()),
        Metadata.IntSlot.values(),
        NONE,
        Metadata.RangeSlot.values(),
        Metadata.FlagBit.values()),
    TEXT(8, columns(Text.Col.values()), NONE, NONE, NONE, NONE);

    private final int number;
    private final List<Col> columns;
    private final List<Enum<?>> ints;
    private final List<Enum<?>> floats;
    private final List<Enum<?>> ranges;
    private final List<Enum<?>> flags;
    private final @Nullable Class<?> intType;
    private final @Nullable Class<?> floatType;

    Kind(
        int number,
        Col[] columns,
        Enum<?>[] ints,
        Enum<?>[] floats,
        Enum<?>[] ranges,
        Enum<?>[] flags) {
      this.number = number;
      this.columns = List.of(columns);
      this.ints = List.of(ints);
      this.floats = List.of(floats);
      this.ranges = List.of(ranges);
      this.flags = List.of(flags);
      this.intType = ints.length == 0 ? null : ints[0].getDeclaringClass();
      this.floatType = floats.length == 0 ? null : floats[0].getDeclaringClass();
    }

    /** The common columns, then each group of a kind's own, in order. */
    private static Col[] columns(Col[]... own) {
      List<Col> all = new ArrayList<>(List.of(Common.values()));
      for (Col[] group : own) {
        all.addAll(List.of(group));
      }
      return all.toArray(Col[]::new);
    }

    /** Throw unless {@code slot} is one of this kind's {@code Ints} slots. */
    int intSlot(Enum<?> slot) {
      return slot(slot, intType, "int");
    }

    /** Throw unless {@code slot} is one of this kind's {@code Floats} slots. */
    int floatSlot(Enum<?> slot) {
      return slot(slot, floatType, "float");
    }

    private int slot(Enum<?> slot, @Nullable Class<?> type, String what) {
      if (slot.getDeclaringClass() != type) {
        throw new IllegalArgumentException(
            slot.getDeclaringClass().getName()
                + "."
                + slot
                + " is not a "
                + what
                + " slot of "
                + this);
      }
      return slot.ordinal();
    }

    /** The native kind number. */
    public int number() {
      return number;
    }

    /** The number of columns. */
    int columnCount() {
      return columns.size();
    }

    /** The column at {@code index}. */
    Col column(int index) {
      return columns.get(index);
    }

    /** The number of {@code Ints} slots. */
    int intCount() {
      return ints.size();
    }

    /** The number of {@code Floats} slots. */
    int floatCount() {
      return floats.size();
    }

    /** The schema text this jar expects, in {@code bolivar_schema}'s format. */
    public String expected() {
      List<String> lines = new ArrayList<>();
      for (Col column : columns) {
        lines.add("col:" + column.name() + ":" + column.elem().name());
      }
      add(lines, "int:", ints);
      add(lines, "float:", floats);
      add(lines, "range:", ranges);
      add(lines, "flag:", flags);
      return String.join("\n", lines);
    }

    private static void add(List<String> lines, String prefix, List<Enum<?>> names) {
      for (Enum<?> name : names) {
        lines.add(prefix + name.name());
      }
    }
  }

  /**
   * Check every kind against the native schema text. The raw kind's expected text is the layout
   * columns followed by the raw-only ones, so this also checks that native raw batches start with
   * the layout columns.
   *
   * @throws IllegalStateException naming the first line that differs
   */
  public static void verify(IntFunction<String> nativeSchema) {
    for (Kind kind : Kind.values()) {
      Contract.require(
          "batch kind " + kind.number + " (" + kind + ")",
          kind.expected(),
          nativeSchema.apply(kind.number));
    }
  }
}
