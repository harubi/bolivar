package sa.ingenious.pdf;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.stream.Collectors;
import org.jspecify.annotations.Nullable;

/** A table and its cells; boxes are in PDF space. */
public record Table(
    int pageNumber, BoundingBox bbox, int rowCount, int columnCount, List<TableCell> cells) {

  public List<TableCell> cellsInRow(int rowIndex) {
    return cells.stream().filter(cell -> cell.rowIndex() == rowIndex).toList();
  }

  public List<TableCell> cellsInColumn(int columnIndex) {
    return cells.stream().filter(cell -> cell.columnIndex() == columnIndex).toList();
  }

  /** The cell starting at a row and column, or null. */
  public @Nullable TableCell cell(int rowIndex, int columnIndex) {
    for (TableCell cell : cells) {
      if (cell.rowIndex() == rowIndex && cell.columnIndex() == columnIndex) {
        return cell;
      }
    }
    return null;
  }

  /** The table as rows of strings; a spanning cell fills every slot it covers. */
  public List<List<String>> toGrid() {
    if (rowCount <= 0 || columnCount <= 0) {
      return List.of();
    }
    List<List<String>> grid = new ArrayList<>(rowCount);
    for (int r = 0; r < rowCount; r++) {
      grid.add(new ArrayList<>(Collections.nCopies(columnCount, "")));
    }
    for (TableCell cell : cells) {
      int rowEnd = Math.min(cell.rowIndex() + Math.max(cell.rowSpan(), 1), rowCount);
      int columnEnd = Math.min(cell.columnIndex() + Math.max(cell.columnSpan(), 1), columnCount);
      for (int r = cell.rowIndex(); r < rowEnd; r++) {
        for (int c = cell.columnIndex(); c < columnEnd; c++) {
          grid.get(r).set(c, cell.text());
        }
      }
    }
    return grid.stream().map(List::copyOf).toList();
  }

  /** The grid as CSV with every value quoted. */
  public String toCsv() {
    return toGrid().stream()
        .map(row ->
            row.stream()
                .map(value -> "\"" + value.replace("\"", "\"\"") + "\"")
                .collect(Collectors.joining(",")))
        .collect(Collectors.joining("\n"));
  }
}
