package sa.ingenious.pdf

import kotlinx.coroutines.runBlocking
import java.util.concurrent.TimeUnit
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class DocumentBehaviorTest {
    @Test
    fun extractTextSyncSuspendAndFutureDelegateToBackend() {
        var calls = 0
        val backend =
            object : DocumentBackend {
                override fun extractText(): String {
                    calls += 1
                    return "hello"
                }

                override fun extractPageSummaries(): List<PageSummary> = emptyList()

                override fun extractLayoutPages(): List<LayoutPage> = emptyList()

                override fun extractRawDocument(): RawDocument = RawDocument(0, 0, emptyList())

                override fun extractTables(): List<Table> = emptyList()

                override fun close() {}
            }

        val document = Document.fromBackend(backend)

        assertEquals("hello", document.extractText())

        runBlocking {
            assertEquals("hello", document.extractTextSuspending())
        }

        assertEquals("hello", document.extractTextAsync().get(1, TimeUnit.SECONDS))
        assertEquals(3, calls)
    }

    @Test
    fun topLevelKotlinHelpersUseDocumentApi() {
        val options =
            documentOptions {
                pages(1)
                layout { wordMargin = 0.2 }
            }

        assertEquals(listOf(1), options.pageNumbers)
        assertEquals(0.2, options.layout?.wordMargin)
    }

    @Test
    fun nonNativeExceptionsAreTranslatedToPdfError() {
        val backend =
            object : DocumentBackend {
                override fun extractText(): String = throw IllegalStateException("backend exploded")

                override fun extractPageSummaries(): List<PageSummary> = emptyList()

                override fun extractLayoutPages(): List<LayoutPage> = emptyList()

                override fun extractRawDocument(): RawDocument = RawDocument(0, 0, emptyList())

                override fun extractTables(): List<Table> = emptyList()

                override fun close() {}
            }

        val document = Document.fromBackend(backend)

        assertFailsWith<PdfException.NativeError> {
            document.extractText()
        }
    }

    @Test
    fun closeDelegatesToBackend() {
        var closed = false
        val backend =
            object : DocumentBackend {
                override fun extractText(): String = "ok"

                override fun extractPageSummaries(): List<PageSummary> = emptyList()

                override fun extractLayoutPages(): List<LayoutPage> = emptyList()

                override fun extractRawDocument(): RawDocument = RawDocument(0, 0, emptyList())

                override fun extractTables(): List<Table> = emptyList()

                override fun close() {
                    closed = true
                }
            }

        val document = Document.fromBackend(backend)
        document.close()

        assertEquals(true, closed)
    }
}
