package sa.ingenious.pdf

import java.nio.file.Files
import java.nio.file.Path
import java.util.Properties
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class NativeIntegrationTest {
    @Test
    fun overrideLibraryHasTheExpectedVersionAndSupportsCursors() {
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
