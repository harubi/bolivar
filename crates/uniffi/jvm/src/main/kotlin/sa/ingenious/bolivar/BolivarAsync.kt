package sa.ingenious.bolivar

import sa.ingenious.bolivar.LayoutPage
import sa.ingenious.bolivar.PageSummary
import sa.ingenious.bolivar.Table
import java.util.concurrent.CompletableFuture
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.future.future

object BolivarAsync {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    @JvmStatic
    fun shutdown() {
        scope.cancel("BolivarAsync shutdown")
    }

    @JvmStatic
    fun extractTextFromBytesAsync(
        pdfData: ByteArray,
        password: String?,
    ): CompletableFuture<String> {
        return scope.future {
            sa.ingenious.bolivar.extractTextFromBytesAsync(pdfData, password)
        }
    }

    @JvmStatic
    fun extractTextFromBytesWithPageRangeAsync(
        pdfData: ByteArray,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): CompletableFuture<String> {
        return scope.future {
            sa.ingenious.bolivar.extractTextFromBytesWithPageRangeAsync(pdfData, password, pageNumbers, maxPages)
        }
    }

    @JvmStatic
    fun extractTextFromPathAsync(path: String, password: String?): CompletableFuture<String> {
        return scope.future {
            sa.ingenious.bolivar.extractTextFromPathAsync(path, password)
        }
    }

    @JvmStatic
    fun extractTextFromPathWithPageRangeAsync(
        path: String,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): CompletableFuture<String> {
        return scope.future {
            sa.ingenious.bolivar.extractTextFromPathWithPageRangeAsync(path, password, pageNumbers, maxPages)
        }
    }

    @JvmStatic
    fun extractPageSummariesFromBytesAsync(
        pdfData: ByteArray,
        password: String?,
    ): CompletableFuture<List<PageSummary>> {
        return scope.future {
            sa.ingenious.bolivar.extractPageSummariesFromBytesAsync(pdfData, password)
        }
    }

    @JvmStatic
    fun extractPageSummariesFromBytesWithPageRangeAsync(
        pdfData: ByteArray,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): CompletableFuture<List<PageSummary>> {
        return scope.future {
            sa.ingenious.bolivar.extractPageSummariesFromBytesWithPageRangeAsync(pdfData, password, pageNumbers, maxPages)
        }
    }

    @JvmStatic
    fun extractPageSummariesFromPathAsync(
        path: String,
        password: String?,
    ): CompletableFuture<List<PageSummary>> {
        return scope.future {
            sa.ingenious.bolivar.extractPageSummariesFromPathAsync(path, password)
        }
    }

    @JvmStatic
    fun extractPageSummariesFromPathWithPageRangeAsync(
        path: String,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): CompletableFuture<List<PageSummary>> {
        return scope.future {
            sa.ingenious.bolivar.extractPageSummariesFromPathWithPageRangeAsync(path, password, pageNumbers, maxPages)
        }
    }

    @JvmStatic
    fun extractLayoutPagesFromBytesAsync(
        pdfData: ByteArray,
        password: String?,
    ): CompletableFuture<List<LayoutPage>> {
        return scope.future {
            sa.ingenious.bolivar.extractLayoutPagesFromBytesAsync(pdfData, password)
        }
    }

    @JvmStatic
    fun extractLayoutPagesFromBytesWithPageRangeAsync(
        pdfData: ByteArray,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): CompletableFuture<List<LayoutPage>> {
        return scope.future {
            sa.ingenious.bolivar.extractLayoutPagesFromBytesWithPageRangeAsync(pdfData, password, pageNumbers, maxPages)
        }
    }

    @JvmStatic
    fun extractLayoutPagesFromPathAsync(
        path: String,
        password: String?,
    ): CompletableFuture<List<LayoutPage>> {
        return scope.future {
            sa.ingenious.bolivar.extractLayoutPagesFromPathAsync(path, password)
        }
    }

    @JvmStatic
    fun extractLayoutPagesFromPathWithPageRangeAsync(
        path: String,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): CompletableFuture<List<LayoutPage>> {
        return scope.future {
            sa.ingenious.bolivar.extractLayoutPagesFromPathWithPageRangeAsync(path, password, pageNumbers, maxPages)
        }
    }

    @JvmStatic
    fun extractTablesFromBytesAsync(
        pdfData: ByteArray,
        password: String?,
    ): CompletableFuture<List<Table>> {
        return scope.future {
            sa.ingenious.bolivar.extractTablesFromBytesAsync(pdfData, password)
        }
    }

    @JvmStatic
    fun extractTablesFromBytesWithPageRangeAsync(
        pdfData: ByteArray,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): CompletableFuture<List<Table>> {
        return scope.future {
            sa.ingenious.bolivar.extractTablesFromBytesWithPageRangeAsync(pdfData, password, pageNumbers, maxPages)
        }
    }

    @JvmStatic
    fun extractTablesFromPathAsync(path: String, password: String?): CompletableFuture<List<Table>> {
        return scope.future {
            sa.ingenious.bolivar.extractTablesFromPathAsync(path, password)
        }
    }

    @JvmStatic
    fun extractTablesFromPathWithPageRangeAsync(
        path: String,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): CompletableFuture<List<Table>> {
        return scope.future {
            sa.ingenious.bolivar.extractTablesFromPathWithPageRangeAsync(path, password, pageNumbers, maxPages)
        }
    }
}
