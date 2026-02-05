package io.bolivar.jvm

import io.bolivar.LayoutPage
import io.bolivar.PageSummary
import io.bolivar.Table
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
            io.bolivar.`extractTextFromBytesAsync`(pdfData, password)
        }
    }

    @JvmStatic
    fun extractTextFromPathAsync(path: String, password: String?): CompletableFuture<String> {
        return scope.future {
            io.bolivar.`extractTextFromPathAsync`(path, password)
        }
    }

    @JvmStatic
    fun extractPageSummariesFromBytesAsync(
        pdfData: ByteArray,
        password: String?,
    ): CompletableFuture<List<PageSummary>> {
        return scope.future {
            io.bolivar.`extractPageSummariesFromBytesAsync`(pdfData, password)
        }
    }

    @JvmStatic
    fun extractPageSummariesFromPathAsync(
        path: String,
        password: String?,
    ): CompletableFuture<List<PageSummary>> {
        return scope.future {
            io.bolivar.`extractPageSummariesFromPathAsync`(path, password)
        }
    }

    @JvmStatic
    fun extractLayoutPagesFromBytesAsync(
        pdfData: ByteArray,
        password: String?,
    ): CompletableFuture<List<LayoutPage>> {
        return scope.future {
            io.bolivar.`extractLayoutPagesFromBytesAsync`(pdfData, password)
        }
    }

    @JvmStatic
    fun extractLayoutPagesFromPathAsync(
        path: String,
        password: String?,
    ): CompletableFuture<List<LayoutPage>> {
        return scope.future {
            io.bolivar.`extractLayoutPagesFromPathAsync`(path, password)
        }
    }

    @JvmStatic
    fun extractTablesFromBytesAsync(
        pdfData: ByteArray,
        password: String?,
    ): CompletableFuture<List<Table>> {
        return scope.future {
            io.bolivar.`extractTablesFromBytesAsync`(pdfData, password)
        }
    }

    @JvmStatic
    fun extractTablesFromPathAsync(path: String, password: String?): CompletableFuture<List<Table>> {
        return scope.future {
            io.bolivar.`extractTablesFromPathAsync`(path, password)
        }
    }
}
