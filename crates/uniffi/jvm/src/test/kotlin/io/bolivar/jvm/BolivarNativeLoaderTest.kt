package sa.ingenious.bolivar

import kotlin.test.Test
import kotlin.test.assertContains
import kotlin.test.assertTrue

class BolivarNativeLoaderTest {
    @Test
    fun defaultResourcePathContainsClassifierAndMappedLibraryName() {
        val path = BolivarNativeLoader.defaultResourcePath()
        val classifier = BolivarNativeLoader.currentClassifier()

        assertContains(path, classifier)
        assertTrue(path.endsWith(System.mapLibraryName("bolivar_uniffi")))
    }
}
