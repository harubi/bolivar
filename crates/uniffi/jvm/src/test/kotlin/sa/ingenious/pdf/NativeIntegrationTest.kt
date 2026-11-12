package sa.ingenious.pdf

import java.nio.file.Files
import java.nio.file.Path
import java.util.Properties
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class NativeIntegrationTest {
    private fun useTestNativeLibrary() {
        val nativeLibrary =
            Path
                .of(
                    "natives",
                    NativeLibrary.currentClassifier(),
                    System.mapLibraryName(NativeLibrary.LIB_NAME),
                ).toAbsolutePath()
                .normalize()
        assertTrue(Files.isRegularFile(nativeLibrary), "Missing test native library: $nativeLibrary")
        NativeLibrary.configureLibraryOverride(nativeLibrary.toString())
    }

    @Test
    fun pageTextsYieldEveryPage() {
        useTestNativeLibrary()
        val fixture =
            Path.of("../../core/tests/fixtures/simple1.pdf")
                .toAbsolutePath()
                .normalize()
        Document.open(fixture).use { document ->
            val raw = document.extractRawDocument()
            val texts: List<PageText> =
                document.pageTexts().use { cursor ->
                    buildList { while (cursor.hasNext()) add(cursor.next()) }
                }
            assertEquals(raw.pages.map { it.pageNumber }, texts.map { it.pageNumber })
            texts.zip(raw.pages).forEach { (text, page) ->
                assertEquals(page.layoutBbox, text.bbox)
                assertEquals(page.textBoxes.map { it.bbox }, text.textBoxes.map { it.bbox })
                assertEquals(page.textBoxes.map { it.text }, text.textBoxes.map { it.text })
                assertEquals(
                    page.textBoxes.flatMap { box -> box.lines.map { it.text } },
                    text.textBoxes.flatMap { box -> box.lines.map { it.text } },
                )
                val visible =
                    page.textBoxes
                        .flatMap { box -> box.lines.flatMap { it.characters } }
                        .filterNot { it.text.isBlank() }
                assertEquals(visible.map { it.fontName }.toSortedSet().toList(), text.fonts)
            }
            assertTrue(texts.any { it.textBoxes.isNotEmpty() })
        }
    }

    @Test
    fun overrideLibraryHasTheExpectedVersionAndSupportsCursors() {
        useTestNativeLibrary()

        val version = Properties()
        val versionResource =
            checkNotNull(
                NativeIntegrationTest::class.java
                    .getResourceAsStream("/bolivar-version.properties"),
            )
        versionResource.use(version::load)
        assertEquals(version.getProperty("version"), Document.version())

        val fixture =
            Path.of("../../core/tests/fixtures/simple1.pdf")
                .toAbsolutePath()
                .normalize()
        Document.open(fixture).use { document ->
            document.pageSummaries().use { pages ->
                assertTrue(pages.hasNext())
                assertEquals(1, pages.next().pageNumber)
                assertFalse(pages.hasNext())
            }
            document.tableRows(TableOptions()).use { rows ->
                assertTrue(rows.hasNext())
                assertEquals(1, rows.next().pageNumber)
                assertFalse(rows.hasNext())
            }
            document.tables().use { tables ->
                while (tables.hasNext()) {
                    tables.next()
                }
            }
        }
    }
}
