package sa.ingenious.pdf

import sa.ingenious.ffi.NativePageTableRowsCursor
import sa.ingenious.ffi.NativePdfDocument
import sa.ingenious.ffi.NativeTableCursor

internal class NativeDocumentBackend(
    private val native: NativePdfDocument,
) : DocumentBackend {
    override fun extractText(): String = native.extractText()

    override fun extractPageSummaries(): List<PageSummary> = native.extractPageSummaries().map { it.toPublic() }

    override fun extractLayoutPages(): List<LayoutPage> = native.extractLayoutPages().map { it.toPublic() }

    override fun extractRawDocument(): RawDocument = native.extractRawDocument().toPublic()

    override fun extractRawPage(pageNumber: Int): RawPage = native.extractRawPage(pageNumber.toPageNumberUInt()).toPublic()

    override fun metadata(): RawDocumentMetadata = native.metadata().toPublic()

    override fun tables(options: TableOptions?): CursorBackend<Table> =
        NativeTableCursorBackend(native.tables(options?.toNative()))

    override fun tableRows(options: TableOptions?): CursorBackend<PageTableRows> =
        NativePageTableRowsCursorBackend(native.tableRows(options?.toNative()))

    override fun close() {
        native.close()
    }
}

private class NativeTableCursorBackend(
    private val native: NativeTableCursor,
) : CursorBackend<Table> {
    override fun next(): Table? = withPdfExceptions { native.next()?.toPublic() }

    override fun cancel() = withPdfExceptions { native.cancel() }

    override fun close() = withPdfExceptions { native.close() }
}

private class NativePageTableRowsCursorBackend(
    private val native: NativePageTableRowsCursor,
) : CursorBackend<PageTableRows> {
    override fun next(): PageTableRows? =
        withPdfExceptions {
            native.next()?.let {
                PageTableRows(pageNumber = it.pageNumber.toInt(), tables = it.tables)
            }
        }

    override fun cancel() = withPdfExceptions { native.cancel() }

    override fun close() = withPdfExceptions { native.close() }
}
