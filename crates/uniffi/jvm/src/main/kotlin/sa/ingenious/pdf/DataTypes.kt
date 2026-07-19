package sa.ingenious.pdf

import kotlin.math.max
import kotlin.math.min
import sa.ingenious.ffi.BoundingBox as NativeBoundingBox
import sa.ingenious.ffi.LayoutChar as NativeLayoutChar
import sa.ingenious.ffi.LayoutLine as NativeLayoutLine
import sa.ingenious.ffi.LayoutPage as NativeLayoutPage
import sa.ingenious.ffi.LayoutTextBox as NativeLayoutTextBox
import sa.ingenious.ffi.PageSummary as NativePageSummary
import sa.ingenious.ffi.Table as NativeTable
import sa.ingenious.ffi.TableCell as NativeTableCell

@JvmRecord
data class PageTableRows(
    val pageNumber: Int,
    val tables: List<List<List<String?>>>,
)

@JvmRecord
data class BoundingBox(
    val x0: Double,
    val y0: Double,
    val x1: Double,
    val y1: Double,
)

@JvmRecord
data class PageSummary(
    val pageNumber: Int,
    val text: String,
    val bbox: BoundingBox,
    val rotate: Double,
)

@JvmRecord
data class LayoutChar(
    val text: String,
    val bbox: BoundingBox,
    val fontName: String,
    val size: Double,
    val upright: Boolean,
)

@JvmRecord
data class LayoutLine(
    val bbox: BoundingBox,
    val orientation: String,
    val text: String,
    val chars: List<LayoutChar>,
)

@JvmRecord
data class LayoutTextBox(
    val bbox: BoundingBox,
    val writingMode: String,
    val text: String,
    val lines: List<LayoutLine>,
)

@JvmRecord
data class LayoutPage(
    val pageNumber: Int,
    val bbox: BoundingBox,
    val rotate: Double,
    val text: String,
    val textBoxes: List<LayoutTextBox>,
)

@JvmRecord
data class TableCell(
    val rowIndex: Int,
    val columnIndex: Int,
    val rowSpan: Int,
    val columnSpan: Int,
    val bbox: BoundingBox,
    val text: String,
)

@JvmRecord
data class Table(
    val pageNumber: Int,
    val bbox: BoundingBox,
    val rowCount: Int,
    val columnCount: Int,
    val cells: List<TableCell>,
)

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

val LayoutPage.allLines: List<LayoutLine>
    get() = textBoxes.flatMap { it.lines }

val LayoutPage.allChars: List<LayoutChar>
    get() = allLines.flatMap { it.chars }

fun Table.cellsInRow(rowIndex: Int): List<TableCell> {
    require(rowIndex >= 0) { "rowIndex must be >= 0" }
    return cells.filter { it.rowIndex == rowIndex }
}

fun Table.cellsInColumn(columnIndex: Int): List<TableCell> {
    require(columnIndex >= 0) { "columnIndex must be >= 0" }
    return cells.filter { it.columnIndex == columnIndex }
}

operator fun Table.get(
    rowIndex: Int,
    columnIndex: Int,
): TableCell? {
    require(rowIndex >= 0) { "rowIndex must be >= 0" }
    require(columnIndex >= 0) { "columnIndex must be >= 0" }
    return cells.firstOrNull {
        it.rowIndex == rowIndex && it.columnIndex == columnIndex
    }
}

fun Table.toGrid(): List<List<String>> {
    if (rowCount <= 0 || columnCount <= 0) {
        return emptyList()
    }

    val grid = MutableList(rowCount) { MutableList(columnCount) { "" } }
    for (cell in cells) {
        val rowSpan = cell.rowSpan.coerceAtLeast(1)
        val columnSpan = cell.columnSpan.coerceAtLeast(1)
        for (row in cell.rowIndex until (cell.rowIndex + rowSpan).coerceAtMost(rowCount)) {
            for (column in cell.columnIndex until (cell.columnIndex + columnSpan).coerceAtMost(columnCount)) {
                grid[row][column] = cell.text
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

internal fun NativeBoundingBox.toPublic(): BoundingBox =
    BoundingBox(
        x0 = x0,
        y0 = y0,
        x1 = x1,
        y1 = y1,
    )

internal fun NativePageSummary.toPublic(): PageSummary =
    PageSummary(
        pageNumber = pageNumber.toInt(),
        text = text,
        bbox = bbox.toPublic(),
        rotate = rotate,
    )

internal fun NativeLayoutChar.toPublic(): LayoutChar =
    LayoutChar(
        text = text,
        bbox = bbox.toPublic(),
        fontName = fontName,
        size = size,
        upright = upright,
    )

internal fun NativeLayoutLine.toPublic(): LayoutLine =
    LayoutLine(
        bbox = bbox.toPublic(),
        orientation = orientation,
        text = text,
        chars = chars.map { it.toPublic() },
    )

internal fun NativeLayoutTextBox.toPublic(): LayoutTextBox =
    LayoutTextBox(
        bbox = bbox.toPublic(),
        writingMode = writingMode,
        text = text,
        lines = lines.map { it.toPublic() },
    )

internal fun NativeLayoutPage.toPublic(): LayoutPage =
    LayoutPage(
        pageNumber = pageNumber.toInt(),
        bbox = bbox.toPublic(),
        rotate = rotate,
        text = text,
        textBoxes = textBoxes.map { it.toPublic() },
    )

internal fun NativeTableCell.toPublic(): TableCell =
    TableCell(
        rowIndex = rowIndex.toInt(),
        columnIndex = columnIndex.toInt(),
        rowSpan = rowSpan.toInt(),
        columnSpan = columnSpan.toInt(),
        bbox = bbox.toPublic(),
        text = text,
    )

internal fun NativeTable.toPublic(): Table =
    Table(
        pageNumber = pageNumber.toInt(),
        bbox = bbox.toPublic(),
        rowCount = rowCount.toInt(),
        columnCount = columnCount.toInt(),
        cells = cells.map { it.toPublic() },
    )

internal fun Int.toPageNumberUInt(): UInt {
    require(this > 0) { "Page numbers are 1-based; expected > 0 but got $this" }
    return toUInt()
}

internal fun Iterable<Int>.toPageNumbersUInt(): List<UInt> = map { it.toPageNumberUInt() }
