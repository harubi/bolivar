package sa.ingenious.bolivar

import kotlin.math.max
import kotlin.math.min

public typealias BoundingBox = sa.ingenious.bolivar.ffi.BoundingBox
public typealias PageSummary = sa.ingenious.bolivar.ffi.PageSummary
public typealias LayoutChar = sa.ingenious.bolivar.ffi.LayoutChar
public typealias LayoutLine = sa.ingenious.bolivar.ffi.LayoutLine
public typealias LayoutTextBox = sa.ingenious.bolivar.ffi.LayoutTextBox
public typealias LayoutPage = sa.ingenious.bolivar.ffi.LayoutPage
public typealias TableCell = sa.ingenious.bolivar.ffi.TableCell
public typealias Table = sa.ingenious.bolivar.ffi.Table

val PageSummary.pageNumberInt: Int
    get() = pageNumber.toInt()

val LayoutPage.pageNumberInt: Int
    get() = pageNumber.toInt()

val Table.pageNumberInt: Int
    get() = pageNumber.toInt()

val LayoutPage.allLines: List<LayoutLine>
    get() = textBoxes.flatMap { it.lines }

val LayoutPage.allChars: List<LayoutChar>
    get() = allLines.flatMap { it.chars }

fun BoundingBox.width(): Double = x1 - x0

fun BoundingBox.height(): Double = y1 - y0

fun BoundingBox.area(): Double = width() * height()

fun BoundingBox.contains(
    x: Double,
    y: Double,
): Boolean = x in x0..x1 && y in y0..y1

fun BoundingBox.intersects(other: BoundingBox): Boolean {
    val left = max(x0, other.x0)
    val right = min(x1, other.x1)
    val bottom = max(y0, other.y0)
    val top = min(y1, other.y1)
    return left <= right && bottom <= top
}

fun Table.cellsInRow(rowIndex: Int): List<TableCell> {
    require(rowIndex >= 0) { "rowIndex must be >= 0" }
    return cells.filter { it.rowIndex.toInt() == rowIndex }
}

fun Table.cellsInColumn(columnIndex: Int): List<TableCell> {
    require(columnIndex >= 0) { "columnIndex must be >= 0" }
    return cells.filter { it.columnIndex.toInt() == columnIndex }
}

operator fun Table.get(
    rowIndex: Int,
    columnIndex: Int,
): TableCell? {
    require(rowIndex >= 0) { "rowIndex must be >= 0" }
    require(columnIndex >= 0) { "columnIndex must be >= 0" }
    return cells.firstOrNull {
        it.rowIndex.toInt() == rowIndex && it.columnIndex.toInt() == columnIndex
    }
}

fun Table.toGrid(): List<List<String>> {
    val rows = rowCount.toInt()
    val cols = columnCount.toInt()
    if (rows <= 0 || cols <= 0) {
        return emptyList()
    }

    val grid = MutableList(rows) { MutableList(cols) { "" } }
    for (cell in cells) {
        val startRow = cell.rowIndex.toInt()
        val startCol = cell.columnIndex.toInt()
        val rowSpan = cell.rowSpan.toInt().coerceAtLeast(1)
        val colSpan = cell.columnSpan.toInt().coerceAtLeast(1)
        for (r in startRow until (startRow + rowSpan).coerceAtMost(rows)) {
            for (c in startCol until (startCol + colSpan).coerceAtMost(cols)) {
                grid[r][c] = cell.text
            }
        }
    }

    return grid
}

fun Table.toCsv(): String =
    toGrid()
        .joinToString("\n") { row ->
            row.joinToString(",") { value ->
                val escaped = value.replace("\"", "\"\"")
                "\"$escaped\""
            }
        }

internal fun Int.toPageNumberUInt(): UInt {
    require(this > 0) { "Page numbers are 1-based; expected > 0 but got $this" }
    return toUInt()
}

internal fun Iterable<Int>.toPageNumbersUInt(): List<UInt> = map { it.toPageNumberUInt() }
