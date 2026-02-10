package sa.ingenious.bolivar

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import sa.ingenious.bolivar.ffi.NativePdfDocument
import java.io.File
import java.io.InputStream
import java.nio.file.Path
import java.util.concurrent.CompletableFuture
import java.util.concurrent.Executor
import java.util.function.Supplier

object Bolivar {
    @JvmStatic
    fun loadNativeLibrary(): String = BolivarNativeLoader.loadFromClasspath()

    @JvmStatic
    @JvmOverloads
    @Throws(BolivarException::class)
    fun open(
        path: String,
        options: DocumentOptions = DocumentOptions(),
    ): PdfDocument =
        withTranslatedExceptions {
            loadNativeLibrary()
            PdfDocument(NativePdfDocument.fromPath(path, options.toNative()))
        }

    fun open(
        path: String,
        configure: DocumentOptions.Builder.() -> Unit,
    ): PdfDocument = open(path, DocumentOptions.build(configure))

    @JvmStatic
    @JvmOverloads
    @Throws(BolivarException::class)
    fun open(
        path: Path,
        options: DocumentOptions = DocumentOptions(),
    ): PdfDocument = open(path.toString(), options)

    @JvmStatic
    @JvmOverloads
    @Throws(BolivarException::class)
    fun open(
        file: File,
        options: DocumentOptions = DocumentOptions(),
    ): PdfDocument = open(file.toPath(), options)

    @JvmStatic
    @JvmOverloads
    @Throws(BolivarException::class)
    fun open(
        pdfData: ByteArray,
        options: DocumentOptions = DocumentOptions(),
    ): PdfDocument =
        withTranslatedExceptions {
            loadNativeLibrary()
            PdfDocument(NativePdfDocument.fromBytes(pdfData.copyOf(), options.toNative()))
        }

    fun open(
        pdfData: ByteArray,
        configure: DocumentOptions.Builder.() -> Unit,
    ): PdfDocument = open(pdfData, DocumentOptions.build(configure))

    @JvmStatic
    @JvmOverloads
    @Throws(BolivarException::class)
    fun open(
        inputStream: InputStream,
        options: DocumentOptions = DocumentOptions(),
    ): PdfDocument =
        withTranslatedExceptions {
            inputStream.use { stream ->
                open(stream.readBytes(), options)
            }
        }

    suspend fun openAsync(
        path: String,
        options: DocumentOptions = DocumentOptions(),
    ): PdfDocument = withContext(Dispatchers.IO) { open(path, options) }

    suspend fun openAsync(
        pdfData: ByteArray,
        options: DocumentOptions = DocumentOptions(),
    ): PdfDocument = withContext(Dispatchers.IO) { open(pdfData, options) }

    @JvmStatic
    @JvmOverloads
    fun openFuture(
        path: String,
        options: DocumentOptions = DocumentOptions(),
        executor: Executor? = null,
    ): CompletableFuture<PdfDocument> = future(executor) { open(path, options) }

    @JvmStatic
    @JvmOverloads
    fun openFuture(
        pdfData: ByteArray,
        options: DocumentOptions = DocumentOptions(),
        executor: Executor? = null,
    ): CompletableFuture<PdfDocument> = future(executor) { open(pdfData, options) }

    @JvmStatic
    @JvmOverloads
    @Throws(BolivarException::class)
    fun extractText(
        path: String,
        options: DocumentOptions = DocumentOptions(),
    ): String = open(path, options).use { doc -> doc.extractText() }

    fun extractText(
        path: String,
        configure: DocumentOptions.Builder.() -> Unit,
    ): String = open(path, configure).use { doc -> doc.extractText() }

    @JvmStatic
    @JvmOverloads
    @Throws(BolivarException::class)
    fun extractText(
        path: Path,
        options: DocumentOptions = DocumentOptions(),
    ): String = open(path, options).use { doc -> doc.extractText() }

    @JvmStatic
    @JvmOverloads
    @Throws(BolivarException::class)
    fun extractText(
        file: File,
        options: DocumentOptions = DocumentOptions(),
    ): String = open(file, options).use { doc -> doc.extractText() }

    @JvmStatic
    @JvmOverloads
    @Throws(BolivarException::class)
    fun extractText(
        pdfData: ByteArray,
        options: DocumentOptions = DocumentOptions(),
    ): String = open(pdfData, options).use { doc -> doc.extractText() }

    fun extractText(
        pdfData: ByteArray,
        configure: DocumentOptions.Builder.() -> Unit,
    ): String = open(pdfData, configure).use { doc -> doc.extractText() }

    @JvmStatic
    @JvmOverloads
    @Throws(BolivarException::class)
    fun extractText(
        inputStream: InputStream,
        options: DocumentOptions = DocumentOptions(),
    ): String = open(inputStream, options).use { doc -> doc.extractText() }

    suspend fun extractTextAsync(
        path: String,
        options: DocumentOptions = DocumentOptions(),
    ): String =
        openAsync(path, options).let { doc ->
            try {
                doc.extractTextAsync()
            } finally {
                doc.close()
            }
        }

    suspend fun extractTextAsync(
        pdfData: ByteArray,
        options: DocumentOptions = DocumentOptions(),
    ): String =
        openAsync(pdfData, options).let { doc ->
            try {
                doc.extractTextAsync()
            } finally {
                doc.close()
            }
        }

    @JvmStatic
    @JvmOverloads
    fun extractTextFuture(
        path: String,
        options: DocumentOptions = DocumentOptions(),
        executor: Executor? = null,
    ): CompletableFuture<String> = future(executor) { extractText(path, options) }

    @JvmStatic
    @JvmOverloads
    fun extractTextFuture(
        pdfData: ByteArray,
        options: DocumentOptions = DocumentOptions(),
        executor: Executor? = null,
    ): CompletableFuture<String> = future(executor) { extractText(pdfData, options) }

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
