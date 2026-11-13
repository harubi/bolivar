package sa.ingenious.pdf.internal;

import java.util.Arrays;
import java.util.List;
import java.util.stream.Collectors;

/**
 * Batch kinds and their columns, mirroring {@code crates/ffi/src/batch.rs}. A column's index is
 * its enum ordinal. {@code NativeContractTest} checks every kind against the native schema text.
 */
public final class Schema {
  private Schema() {}

  public static final int LAYOUT = 1;
  public static final int RAW = 2;
  public static final int PAGE_TEXT = 3;
  public static final int SUMMARY = 4;
  public static final int TABLES = 5;
  public static final int TABLE_ROWS = 6;
  public static final int METADATA = 7;
  public static final int TEXT = 8;

  /** Column element types, as {@code batch::Elem}. */
  public enum Elem {
    F64(1),
    I32(2),
    I64(3),
    U8(4),
    U16(5);

    final int code;

    Elem(int code) {
      this.code = code;
    }

    static Elem of(int code) {
      for (Elem elem : values()) {
        if (elem.code == code) {
          return elem;
        }
      }
      throw new IllegalStateException("unknown column element type " + code);
    }
  }

  /** A column of some kind; implemented by the per-kind enums. */
  public interface Column {
    int ordinal();

    String name();

    Elem elem();
  }

  public enum Layout implements Column {
    Text(Elem.U16), Strings(Elem.U16), StringOff(Elem.I32), Ints(Elem.I64), Floats(Elem.F64),
    BoxBbox(Elem.F64), BoxMode(Elem.I32), BoxText(Elem.I32), BoxLines(Elem.I32),
    LineBbox(Elem.F64), LineOrient(Elem.I32), LineText(Elem.I32), LineChars(Elem.I32),
    CharX0(Elem.F64), CharY0(Elem.F64), CharX1(Elem.F64), CharY1(Elem.F64),
    CharSize(Elem.F64), CharFont(Elem.I32), CharUpright(Elem.U8), CharText(Elem.I32);

    private final Elem elem;

    Layout(Elem elem) {
      this.elem = elem;
    }

    @Override
    public Elem elem() {
      return elem;
    }
  }

  public enum Raw implements Column {
    Text(Elem.U16), Strings(Elem.U16), StringOff(Elem.I32), Ints(Elem.I64), Floats(Elem.F64),
    PageBoxes(Elem.F64), PageBoxPresent(Elem.U8),
    BoxBbox(Elem.F64), BoxMode(Elem.I32), BoxText(Elem.I32), BoxLines(Elem.I32),
    LineBbox(Elem.F64), LineOrient(Elem.I32), LineRawText(Elem.I32), LineText(Elem.I32),
    LineChars(Elem.I32),
    CharX0(Elem.F64), CharY0(Elem.F64), CharX1(Elem.F64), CharY1(Elem.F64),
    CharSize(Elem.F64), CharFont(Elem.I32), CharUpright(Elem.U8), CharText(Elem.I32),
    CharAdvance(Elem.F64), CharMatrix(Elem.F64), CharMcid(Elem.I32),
    CharTag(Elem.I32), CharNcs(Elem.I32), CharScs(Elem.I32),
    CharNcolor(Elem.I32), CharScolor(Elem.I32), ColorPool(Elem.F64),
    TableBbox(Elem.F64), TableShape(Elem.I32), TableCells(Elem.I32),
    CellIndex(Elem.I32), CellBbox(Elem.F64), CellText(Elem.I32);

    private final Elem elem;

    Raw(Elem elem) {
      this.elem = elem;
    }

    @Override
    public Elem elem() {
      return elem;
    }
  }

  public enum PageText implements Column {
    Text(Elem.U16), Strings(Elem.U16), StringOff(Elem.I32), Ints(Elem.I64), Floats(Elem.F64),
    BoxBbox(Elem.F64), BoxText(Elem.I32), BoxTextWithoutBidi(Elem.I32), BoxLines(Elem.I32),
    LineBbox(Elem.F64), LineText(Elem.I32), LineTextWithoutBidi(Elem.I32),
    Fonts(Elem.I32), DigitFont(Elem.I32), DigitSize(Elem.I32);

    private final Elem elem;

    PageText(Elem elem) {
      this.elem = elem;
    }

    @Override
    public Elem elem() {
      return elem;
    }
  }

  public enum Summary implements Column {
    Text(Elem.U16), Strings(Elem.U16), StringOff(Elem.I32), Ints(Elem.I64), Floats(Elem.F64);

    private final Elem elem;

    Summary(Elem elem) {
      this.elem = elem;
    }

    @Override
    public Elem elem() {
      return elem;
    }
  }

  public enum Tables implements Column {
    Text(Elem.U16), Strings(Elem.U16), StringOff(Elem.I32), Ints(Elem.I64), Floats(Elem.F64),
    TableBbox(Elem.F64), TableShape(Elem.I32), TableCells(Elem.I32),
    CellIndex(Elem.I32), CellBbox(Elem.F64), CellText(Elem.I32);

    private final Elem elem;

    Tables(Elem elem) {
      this.elem = elem;
    }

    @Override
    public Elem elem() {
      return elem;
    }
  }

  public enum TableRows implements Column {
    Text(Elem.U16), Strings(Elem.U16), StringOff(Elem.I32), Ints(Elem.I64), Floats(Elem.F64),
    CellText(Elem.I32), RowOffsets(Elem.I32), TableOffsets(Elem.I32);

    private final Elem elem;

    TableRows(Elem elem) {
      this.elem = elem;
    }

    @Override
    public Elem elem() {
      return elem;
    }
  }

  public enum Metadata implements Column {
    Text(Elem.U16), Strings(Elem.U16), StringOff(Elem.I32), Ints(Elem.I64), Floats(Elem.F64),
    Fields(Elem.I32), InfoKeys(Elem.I32), InfoValues(Elem.I32);

    private final Elem elem;

    Metadata(Elem elem) {
      this.elem = elem;
    }

    @Override
    public Elem elem() {
      return elem;
    }
  }

  public enum Text implements Column {
    Text(Elem.U16), Strings(Elem.U16), StringOff(Elem.I32), Ints(Elem.I64), Floats(Elem.F64);

    private final Elem elem;

    Text(Elem elem) {
      this.elem = elem;
    }

    @Override
    public Elem elem() {
      return elem;
    }
  }

  /** Every kind number with its columns. */
  public static Column[] columns(int kind) {
    return switch (kind) {
      case LAYOUT -> Layout.values();
      case RAW -> Raw.values();
      case PAGE_TEXT -> PageText.values();
      case SUMMARY -> Summary.values();
      case TABLES -> Tables.values();
      case TABLE_ROWS -> TableRows.values();
      case METADATA -> Metadata.values();
      case TEXT -> Text.values();
      default -> throw new IllegalArgumentException("unknown batch kind " + kind);
    };
  }

  public static final List<Integer> KINDS =
      List.of(LAYOUT, RAW, PAGE_TEXT, SUMMARY, TABLES, TABLE_ROWS, METADATA, TEXT);

  /** The schema text this jar expects for a kind, in the native format. */
  public static String text(int kind) {
    return Arrays.stream(columns(kind))
        .map(column -> column.name() + ":" + column.elem().name())
        .collect(Collectors.joining("\n"));
  }
}
