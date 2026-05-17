package sa.ingenious.pdf

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
        assertEquals(null, options.layout)
    }

    @Test
    fun kotlinDslBuildsNestedLayoutOptions() {
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

        val layout = options.layout ?: error("layout options missing")
        assertEquals(0.5, layout.lineOverlap)
        assertEquals(2.0, layout.charMargin)
        assertEquals(0.7, layout.lineMargin)
        assertEquals(0.1, layout.wordMargin)
        assertEquals(0.3, layout.boxesFlow)
        assertEquals(true, layout.detectVertical)
        assertEquals(false, layout.allTexts)
    }

    @Test
    fun pageNumbersAreDefensivelyCopiedForJvmConsumers() {
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
            DocumentOptions.builder().maxPages(0).build()
        }
    }
}
