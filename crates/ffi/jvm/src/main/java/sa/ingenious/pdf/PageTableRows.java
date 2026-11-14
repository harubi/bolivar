package sa.ingenious.pdf;

import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Objects;
import java.util.stream.IntStream;
import org.jspecify.annotations.Nullable;

/**
 * The raw table rows of one page, exactly as the pdfplumber-compatible rows pipeline emits them.
 * {@code cells} holds every cell in order, null for an empty cell. Row offsets index cells and
 * table offsets index rows; both start at 0 and end with the total.
 */
public final class PageTableRows {
  private final int pageNumber;
  private final List<@Nullable String> cells;
  private final int[] rowOffsets;
  private final int[] tableOffsets;

  public PageTableRows(
      int pageNumber, List<@Nullable String> cells, int[] rowOffsets, int[] tableOffsets) {
    this(pageNumber, cells.toArray(new String[0]), rowOffsets.clone(), tableOffsets.clone());
  }

  /** Takes ownership of the arrays without copying them. */
  PageTableRows(int pageNumber, @Nullable String[] cells, int[] rowOffsets, int[] tableOffsets) {
    this.pageNumber = pageNumber;
    this.cells =
        Collections.<@Nullable String>unmodifiableList(Arrays.<@Nullable String>asList(cells));
    this.rowOffsets = rowOffsets;
    this.tableOffsets = tableOffsets;
  }

  public int pageNumber() {
    return pageNumber;
  }

  public List<@Nullable String> cells() {
    return cells;
  }

  public int[] rowOffsets() {
    return rowOffsets.clone();
  }

  public int[] tableOffsets() {
    return tableOffsets.clone();
  }

  public int tableCount() {
    return tableOffsets.length - 1;
  }

  /** The rows of table {@code t}, each a list of cells (null for empty). */
  public List<List<@Nullable String>> table(int t) {
    return IntStream.range(tableOffsets[t], tableOffsets[t + 1])
        .mapToObj(r -> cells.subList(rowOffsets[r], rowOffsets[r + 1]))
        .toList();
  }

  @Override
  public boolean equals(@Nullable Object other) {
    return other instanceof PageTableRows rows
        && pageNumber == rows.pageNumber
        && cells.equals(rows.cells)
        && Arrays.equals(rowOffsets, rows.rowOffsets)
        && Arrays.equals(tableOffsets, rows.tableOffsets);
  }

  @Override
  public int hashCode() {
    return Objects.hash(
        pageNumber, cells, Arrays.hashCode(rowOffsets), Arrays.hashCode(tableOffsets));
  }

  @Override
  public String toString() {
    return "PageTableRows[pageNumber="
        + pageNumber
        + ", tables="
        + tableCount()
        + ", cells="
        + cells.size()
        + "]";
  }
}
