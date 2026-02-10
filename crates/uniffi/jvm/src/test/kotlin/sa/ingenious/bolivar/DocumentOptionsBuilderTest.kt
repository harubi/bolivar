package sa.ingenious.bolivar

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class DocumentOptionsBuilderTest {
    @Test
    fun builderDefaultsMatchPublicDefaults() {
        val options = DocumentOptions.builder().build()

        assertEquals(null, options.password)
        assertEquals(null, options.pageNumbers)
        assertEquals(null, options.maxPages)
        assertEquals(true, options.caching)
        assertEquals(null, options.layoutParams)
    }

    @Test
    fun dslBuilderBuildsNestedLayoutParams() {
        val options =
            documentOptions {
                password = "secret"
                pages(1, 3)
                maxPages = 2
                caching = false
                layout {
                    lineOverlap = 0.5
                    charMargin = 2.0
                    lineMargin = 0.7
                    wordMargin = 0.1
                    boxesFlow = 0.3
                    detectVertical = true
                    allTexts = false
                }
            }

        assertEquals("secret", options.password)
        assertEquals(listOf(1, 3), options.pageNumbers)
        assertEquals(2, options.maxPages)
        assertEquals(false, options.caching)

        val layoutParams = options.layoutParams ?: error("layout params missing")
        assertEquals(0.5, layoutParams.lineOverlap)
        assertEquals(2.0, layoutParams.charMargin)
        assertEquals(0.7, layoutParams.lineMargin)
        assertEquals(0.1, layoutParams.wordMargin)
        assertEquals(0.3, layoutParams.boxesFlow)
        assertEquals(true, layoutParams.detectVertical)
        assertEquals(false, layoutParams.allTexts)
    }

    @Test
    fun pageNumbersDefensivelyCopied() {
        val mutablePages = mutableListOf(1, 2)
        val options =
            DocumentOptions
                .builder()
                .pageNumbers(mutablePages)
                .build()

        mutablePages += 9

        assertEquals(listOf(1, 2), options.pageNumbers)
    }

    @Test
    fun optionsRejectInvalidPageNumbers() {
        assertFailsWith<IllegalArgumentException> {
            DocumentOptions.builder().pages(0).build()
        }

        assertFailsWith<IllegalArgumentException> {
            DocumentOptions.builder().apply { maxPages = 0 }.build()
        }
    }
}
