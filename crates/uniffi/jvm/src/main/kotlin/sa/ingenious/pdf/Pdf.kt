package sa.ingenious.pdf

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.io.InputStream
import java.nio.file.Path

@JvmSynthetic
fun openDocument(
    path: String,
    options: DocumentOptions = DocumentOptions(),
): Document = Document.open(path, options)

@JvmSynthetic
fun openDocument(
    path: String,
    configure: DocumentOptions.Dsl.() -> Unit,
): Document = openDocument(path, DocumentOptions.build(configure))

@JvmSynthetic
fun openDocument(
    path: Path,
    options: DocumentOptions = DocumentOptions(),
): Document = Document.open(path, options)

@JvmSynthetic
fun openDocument(
    file: File,
    options: DocumentOptions = DocumentOptions(),
): Document = Document.open(file, options)

@JvmSynthetic
fun openDocument(
    pdfData: ByteArray,
    options: DocumentOptions = DocumentOptions(),
): Document = Document.open(pdfData, options)

@JvmSynthetic
fun openDocument(
    pdfData: ByteArray,
    configure: DocumentOptions.Dsl.() -> Unit,
): Document = openDocument(pdfData, DocumentOptions.build(configure))

@JvmSynthetic
fun openDocument(
    inputStream: InputStream,
    options: DocumentOptions = DocumentOptions(),
): Document = Document.open(inputStream, options)

@JvmSynthetic
fun extractText(
    path: String,
    options: DocumentOptions = DocumentOptions(),
): String = Document.extractText(path, options)

@JvmSynthetic
fun extractText(
    path: String,
    configure: DocumentOptions.Dsl.() -> Unit,
): String = Document.extractText(path, DocumentOptions.build(configure))

@JvmSynthetic
fun extractText(
    path: Path,
    options: DocumentOptions = DocumentOptions(),
): String = Document.extractText(path, options)

@JvmSynthetic
fun extractText(
    file: File,
    options: DocumentOptions = DocumentOptions(),
): String = Document.extractText(file, options)

@JvmSynthetic
fun extractText(
    pdfData: ByteArray,
    options: DocumentOptions = DocumentOptions(),
): String = Document.extractText(pdfData, options)

@JvmSynthetic
fun extractText(
    pdfData: ByteArray,
    configure: DocumentOptions.Dsl.() -> Unit,
): String = Document.extractText(pdfData, DocumentOptions.build(configure))

@JvmSynthetic
fun extractText(
    inputStream: InputStream,
    options: DocumentOptions = DocumentOptions(),
): String = Document.extractText(inputStream, options)

@JvmSynthetic
suspend fun Document.extractTextSuspending(): String = withContext(Dispatchers.IO) { extractText() }
