package sa.ingenious.pdf

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertContains
import kotlin.test.assertTrue

class NativeLibraryTest {
    @Test
    fun defaultResourcePathContainsClassifierAndMappedLibraryName() {
        val path = NativeLibrary.defaultResourcePath()
        val classifier = NativeLibrary.currentClassifier()

        assertContains(path, classifier)
        assertTrue(path.endsWith(System.mapLibraryName("bolivar_uniffi")))
    }

    @Test
    fun classifiersMatchReleaseWorkflowDirectoryNames() {
        assertEquals("linux-x86-64", NativeLibrary.classifierFor("Linux", "x86_64"))
        assertEquals("linux-x86-64", NativeLibrary.classifierFor("Linux", "amd64"))
        assertEquals("linux-aarch64", NativeLibrary.classifierFor("Linux", "aarch64"))
        assertEquals("macos-x86-64", NativeLibrary.classifierFor("Mac OS X", "x86_64"))
        assertEquals("macos-aarch64", NativeLibrary.classifierFor("Mac OS X", "aarch64"))
        assertEquals("windows-x86-64", NativeLibrary.classifierFor("Windows 11", "amd64"))
    }
}
