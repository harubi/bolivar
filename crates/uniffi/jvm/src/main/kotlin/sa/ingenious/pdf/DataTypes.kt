package sa.ingenious.pdf

import kotlin.math.max
import kotlin.math.min
import sa.ingenious.ffi.BoundingBox as NativeBoundingBox
import sa.ingenious.ffi.LayoutChar as NativeLayoutChar
import sa.ingenious.ffi.LayoutLine as NativeLayoutLine
import sa.ingenious.ffi.LayoutPage as NativeLayoutPage
import sa.ingenious.ffi.LayoutTextBox as NativeLayoutTextBox
import sa.ingenious.ffi.MetadataEntry as NativeMetadataEntry
import sa.ingenious.ffi.PageSummary as NativePageSummary
import sa.ingenious.ffi.PdfPermissions as NativePdfPermissions
import sa.ingenious.ffi.PdfVersion as NativePdfVersion
import sa.ingenious.ffi.RawCharacter as NativeRawCharacter
import sa.ingenious.ffi.RawDocument as NativeRawDocument
import sa.ingenious.ffi.RawDocumentMetadata as NativeRawDocumentMetadata
import sa.ingenious.ffi.RawPage as NativeRawPage
import sa.ingenious.ffi.RawPageBoxes as NativeRawPageBoxes
import sa.ingenious.ffi.RawTable as NativeRawTable
import sa.ingenious.ffi.RawTableBoundingBox as NativeRawTableBoundingBox
import sa.ingenious.ffi.RawTableCell as NativeRawTableCell
import sa.ingenious.ffi.RawTextBox as NativeRawTextBox
import sa.ingenious.ffi.RawTextLine as NativeRawTextLine
import sa.ingenious.ffi.Table as NativeTable
import sa.ingenious.ffi.TableCell as NativeTableCell

/**
 * Flat table rows. Row offsets index cells; table offsets index rows.
 * Both offset arrays start at zero and include the final end offset.
 */
@JvmRecord
data class PageTableRows(
    val pageNumber: Int,
    val cells: List<String?>,
    val rowOffsets: IntArray,
    val tableOffsets: IntArray,
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

@JvmRecord
data class RawTableBoundingBox(
    val x0: Double,
    val top: Double,
    val x1: Double,
    val bottom: Double,
)

@JvmRecord
data class RawCharacter(
    val text: String,
    val bbox: BoundingBox,
    val fontName: String,
    val size: Double,
    val upright: Boolean,
    val advance: Double,
    val matrix: List<Double>,
    val markedContentId: Int?,
    val tag: String?,
    val nonStrokingColorSpace: String?,
    val strokingColorSpace: String?,
    val nonStrokingColor: List<Double>?,
    val strokingColor: List<Double>?,
)

@JvmRecord
data class RawTextLine(
    val bbox: BoundingBox,
    val orientation: String,
    val rawText: String,
    val text: String,
    val characters: List<RawCharacter>,
)

@JvmRecord
data class RawTextBox(
    val bbox: BoundingBox,
    val writingMode: String,
    val text: String,
    val lines: List<RawTextLine>,
)

@JvmRecord
data class RawTableCell(
    val rowIndex: Int,
    val columnIndex: Int,
    val rowSpan: Int,
    val columnSpan: Int,
    val bbox: RawTableBoundingBox,
    val text: String,
)

@JvmRecord
data class RawTable(
    val bbox: RawTableBoundingBox,
    val rowCount: Int,
    val columnCount: Int,
    val cells: List<RawTableCell>,
)

@JvmRecord
data class RawPageBoxes(
    val media: List<Double>?,
    val crop: List<Double>?,
    val bleed: List<Double>?,
    val trim: List<Double>?,
    val art: List<Double>?,
)

@JvmRecord
data class RawPage(
    val pageIndex: Int,
    val pageNumber: Int,
    val objectId: Int,
    val label: String?,
    val rotation: Long,
    val userUnit: Double,
    val boxes: RawPageBoxes,
    val layoutBbox: BoundingBox,
    val text: String,
    val textBoxes: List<RawTextBox>,
    val tables: List<RawTable>,
)

@JvmRecord
data class RawDocument(
    val declaredPageCount: Int,
    val pageCount: Int,
    val pages: List<RawPage>,
)

@JvmRecord
data class MetadataEntry(
    val key: String,
    val value: String,
)

@JvmRecord
data class PdfVersion(
    val header: String?,
    val catalog: String?,
    val effective: String?,
)

@JvmRecord
data class PdfPermissions(
    val printable: Boolean,
    val modifiable: Boolean,
    val extractable: Boolean,
)

@JvmRecord
data class RawDocumentMetadata(
    val documentInfo: List<MetadataEntry>,
    val title: String?,
    val author: String?,
    val subject: String?,
    val keywords: String?,
    val creator: String?,
    val producer: String?,
    val creationDateRaw: String?,
    val creationDateIso: String?,
    val modificationDateRaw: String?,
    val modificationDateIso: String?,
    val version: PdfVersion,
    val fileSizeBytes: Long,
    val pageCount: Int,
    val encrypted: Boolean,
    val permissions: PdfPermissions,
    val linearized: Boolean,
    val tagged: Boolean,
    val userProperties: Boolean,
    val suspects: Boolean,
    val form: String,
    val hasJavascript: Boolean,
    val hasMetadataStream: Boolean,
    val xmpMetadata: String?,
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

internal fun NativeRawTableBoundingBox.toPublic(): RawTableBoundingBox =
    RawTableBoundingBox(
        x0 = x0,
        top = top,
        x1 = x1,
        bottom = bottom,
    )

internal fun NativeRawCharacter.toPublic(): RawCharacter =
    RawCharacter(
        text = text,
        bbox = bbox.toPublic(),
        fontName = fontName,
        size = size,
        upright = upright,
        advance = advance,
        matrix = matrix,
        markedContentId = markedContentId,
        tag = tag,
        nonStrokingColorSpace = nonStrokingColorSpace,
        strokingColorSpace = strokingColorSpace,
        nonStrokingColor = nonStrokingColor,
        strokingColor = strokingColor,
    )

internal fun NativeRawTextLine.toPublic(): RawTextLine =
    RawTextLine(
        bbox = bbox.toPublic(),
        orientation = orientation,
        rawText = rawText,
        text = text,
        characters = characters.map { it.toPublic() },
    )

internal fun NativeRawTextBox.toPublic(): RawTextBox =
    RawTextBox(
        bbox = bbox.toPublic(),
        writingMode = writingMode,
        text = text,
        lines = lines.map { it.toPublic() },
    )

internal fun NativeRawTableCell.toPublic(): RawTableCell =
    RawTableCell(
        rowIndex = rowIndex.toInt(),
        columnIndex = columnIndex.toInt(),
        rowSpan = rowSpan.toInt(),
        columnSpan = columnSpan.toInt(),
        bbox = bbox.toPublic(),
        text = text,
    )

internal fun NativeRawTable.toPublic(): RawTable =
    RawTable(
        bbox = bbox.toPublic(),
        rowCount = rowCount.toInt(),
        columnCount = columnCount.toInt(),
        cells = cells.map { it.toPublic() },
    )

internal fun NativeRawPageBoxes.toPublic(): RawPageBoxes =
    RawPageBoxes(
        media = media,
        crop = crop,
        bleed = bleed,
        trim = trim,
        art = art,
    )

internal fun NativeRawPage.toPublic(): RawPage =
    RawPage(
        pageIndex = pageIndex.toInt(),
        pageNumber = pageNumber.toInt(),
        objectId = objectId.toInt(),
        label = label,
        rotation = rotation,
        userUnit = userUnit,
        boxes = boxes.toPublic(),
        layoutBbox = layoutBbox.toPublic(),
        text = text,
        textBoxes = textBoxes.map { it.toPublic() },
        tables = tables.map { it.toPublic() },
    )

internal fun NativeRawDocument.toPublic(): RawDocument =
    RawDocument(
        declaredPageCount = declaredPageCount.toInt(),
        pageCount = pageCount.toInt(),
        pages = pages.map { it.toPublic() },
    )

internal fun NativeMetadataEntry.toPublic(): MetadataEntry = MetadataEntry(key = key, value = value)

internal fun NativePdfVersion.toPublic(): PdfVersion =
    PdfVersion(header = header, catalog = catalog, effective = effective)

internal fun NativePdfPermissions.toPublic(): PdfPermissions =
    PdfPermissions(printable = printable, modifiable = modifiable, extractable = extractable)

internal fun NativeRawDocumentMetadata.toPublic(): RawDocumentMetadata =
    RawDocumentMetadata(
        documentInfo = documentInfo.map { it.toPublic() },
        title = title,
        author = author,
        subject = subject,
        keywords = keywords,
        creator = creator,
        producer = producer,
        creationDateRaw = creationDateRaw,
        creationDateIso = creationDateIso,
        modificationDateRaw = modificationDateRaw,
        modificationDateIso = modificationDateIso,
        version = version.toPublic(),
        fileSizeBytes = fileSizeBytes.toLong(),
        pageCount = pageCount.toInt(),
        encrypted = encrypted,
        permissions = permissions.toPublic(),
        linearized = linearized,
        tagged = tagged,
        userProperties = userProperties,
        suspects = suspects,
        form = form,
        hasJavascript = hasJavascript,
        hasMetadataStream = hasMetadataStream,
        xmpMetadata = xmpMetadata,
    )

internal fun Int.toPageNumberUInt(): UInt {
    require(this > 0) { "Page numbers are 1-based; expected > 0 but got $this" }
    return toUInt()
}

internal fun Iterable<Int>.toPageNumbersUInt(): List<UInt> = map { it.toPageNumberUInt() }
