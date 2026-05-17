package sa.ingenious.pdf

import sa.ingenious.ffi.NativePdfDocument

internal class NativeDocumentBackend(
    private val native: NativePdfDocument,
) : DocumentBackend {
    override fun extractText(): String = native.extractText()

    override fun extractPageSummaries(): List<PageSummary> = native.extractPageSummaries().map { it.toPublic() }

    override fun extractLayoutPages(): List<LayoutPage> = native.extractLayoutPages().map { it.toPublic() }

    override fun extractTables(): List<Table> = native.extractTables().map { it.toPublic() }

    override fun close() {
        native.close()
    }
}
