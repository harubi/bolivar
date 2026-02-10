package sa.ingenious.bolivar

import kotlinx.coroutines.runBlocking
import java.util.concurrent.TimeUnit
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class PdfDocumentBehaviorTest {
    @Test
    fun extractTextSyncAsyncAndFutureDelegateToBackend() {
        var calls = 0
        val backend =
            object : PdfDocumentBackend {
                override fun extractText(): String {
                    calls += 1
                    return "hello"
                }

                override fun extractPageSummaries(): List<PageSummary> = emptyList()

                override fun extractLayoutPages(): List<LayoutPage> = emptyList()

                override fun extractTables(): List<Table> = emptyList()

                override fun close() {}
            }

        val document = PdfDocument(backend)

        assertEquals("hello", document.extractText())

        runBlocking {
            assertEquals("hello", document.extractTextAsync())
        }

        assertEquals("hello", document.extractTextFuture().get(1, TimeUnit.SECONDS))
        assertEquals(3, calls)
    }

    @Test
    fun nonNativeExceptionsAreTranslatedToNativeError() {
        val backend =
            object : PdfDocumentBackend {
                override fun extractText(): String = throw IllegalStateException("backend exploded")

                override fun extractPageSummaries(): List<PageSummary> = emptyList()

                override fun extractLayoutPages(): List<LayoutPage> = emptyList()

                override fun extractTables(): List<Table> = emptyList()

                override fun close() {}
            }

        val document = PdfDocument(backend)

        assertFailsWith<BolivarException.NativeError> {
            document.extractText()
        }
    }

    @Test
    fun closeDelegatesToBackend() {
        var closed = false
        val backend =
            object : PdfDocumentBackend {
                override fun extractText(): String = "ok"

                override fun extractPageSummaries(): List<PageSummary> = emptyList()

                override fun extractLayoutPages(): List<LayoutPage> = emptyList()

                override fun extractTables(): List<Table> = emptyList()

                override fun close() {
                    closed = true
                }
            }

        val document = PdfDocument(backend)
        document.close()

        assertEquals(true, closed)
    }
}
