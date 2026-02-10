package sa.ingenious.bolivar

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import sa.ingenious.bolivar.ffi.NativePdfDocument
import java.util.concurrent.CompletableFuture
import java.util.concurrent.Executor
import java.util.function.Supplier

internal interface PdfDocumentBackend {
    fun extractText(): String

    fun extractPageSummaries(): List<PageSummary>

    fun extractLayoutPages(): List<LayoutPage>

    fun extractTables(): List<Table>

    fun close()
}

internal class NativePdfDocumentBackend(
    private val native: NativePdfDocument,
) : PdfDocumentBackend {
    override fun extractText(): String = native.extractText()

    override fun extractPageSummaries(): List<PageSummary> = native.extractPageSummaries()

    override fun extractLayoutPages(): List<LayoutPage> = native.extractLayoutPages()

    override fun extractTables(): List<Table> = native.extractTables()

    override fun close() {
        native.close()
    }
}

class PdfDocument internal constructor(
    private val backend: PdfDocumentBackend,
) : AutoCloseable {
    internal constructor(native: NativePdfDocument) : this(NativePdfDocumentBackend(native))

    @Throws(BolivarException::class)
    fun extractText(): String = withTranslatedExceptions { backend.extractText() }

    suspend fun extractTextAsync(): String = withContext(Dispatchers.IO) { extractText() }

    @JvmOverloads
    fun extractTextFuture(executor: Executor? = null): CompletableFuture<String> = future(executor) { extractText() }

    @Throws(BolivarException::class)
    fun extractPageSummaries(): List<PageSummary> = withTranslatedExceptions { backend.extractPageSummaries() }

    suspend fun extractPageSummariesAsync(): List<PageSummary> = withContext(Dispatchers.IO) { extractPageSummaries() }

    @JvmOverloads
    fun extractPageSummariesFuture(executor: Executor? = null): CompletableFuture<List<PageSummary>> =
        future(executor) { extractPageSummaries() }

    @Throws(BolivarException::class)
    fun extractLayoutPages(): List<LayoutPage> = withTranslatedExceptions { backend.extractLayoutPages() }

    suspend fun extractLayoutPagesAsync(): List<LayoutPage> = withContext(Dispatchers.IO) { extractLayoutPages() }

    @JvmOverloads
    fun extractLayoutPagesFuture(executor: Executor? = null): CompletableFuture<List<LayoutPage>> =
        future(executor) { extractLayoutPages() }

    @Throws(BolivarException::class)
    fun extractTables(): List<Table> = withTranslatedExceptions { backend.extractTables() }

    suspend fun extractTablesAsync(): List<Table> = withContext(Dispatchers.IO) { extractTables() }

    @JvmOverloads
    fun extractTablesFuture(executor: Executor? = null): CompletableFuture<List<Table>> = future(executor) { extractTables() }

    fun pages(): List<PageSummary> = extractPageSummaries()

    fun tables(): List<Table> = extractTables()

    operator fun get(pageNumber: Int): PageSummary {
        require(pageNumber > 0) { "pageNumber must be >= 1" }
        return extractPageSummaries().firstOrNull { it.pageNumber.toInt() == pageNumber }
            ?: throw BolivarException.InvalidArgument("Page $pageNumber was not extracted")
    }

    operator fun iterator(): Iterator<PageSummary> = extractPageSummaries().iterator()

    override fun close() {
        withTranslatedExceptions {
            backend.close()
        }
    }

    private fun <T> future(
        executor: Executor?,
        block: () -> T,
    ): CompletableFuture<T> {
        val task = Supplier { block() }
        return if (executor == null) {
            CompletableFuture.supplyAsync(task)
        } else {
            CompletableFuture.supplyAsync(task, executor)
        }
    }
}
