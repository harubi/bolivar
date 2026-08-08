package sa.ingenious.pdf

import sa.ingenious.ffi.NativePdfDocument

internal class NativeDocumentBackend(
    private val native: NativePdfDocument,
) : DocumentBackend {
    override fun extractText(): String = native.extractText()

    override fun extractPageSummaries(): List<PageSummary> = native.extractPageSummaries().map { it.toPublic() }

    override fun extractLayoutPages(): List<LayoutPage> = native.extractLayoutPages().map { it.toPublic() }

    override fun extractRawDocument(): RawDocument = native.extractRawDocument().toPublic()

    override fun extractRawPage(pageNumber: Int): RawPage = native.extractRawPage(pageNumber.toPageNumberUInt()).toPublic()

    override fun metadata(): RawDocumentMetadata = native.metadata().toPublic()

    override fun extractTables(): List<Table> = native.extractTables().map { it.toPublic() }

    override fun extractTables(options: TableOptions?): List<Table> =
        native.extractTablesWith(options?.toNative()).map { it.toPublic() }

    override fun extractTableRows(options: TableOptions?): List<PageTableRows> =
        native.extractTableRowsWith(options?.toNative()).map {
            PageTableRows(pageNumber = it.pageNumber.toInt(), tables = it.tables)
        }

    override fun close() {
        native.close()
    }
}
