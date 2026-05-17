package sa.ingenious.pdf

import java.util.function.Consumer
import sa.ingenious.ffi.ExtractOptions as NativeExtractOptions

@JvmRecord
data class DocumentOptions(
    val password: String? = null,
    val pageNumbers: List<Int>? = null,
    val maxPages: Int? = null,
    val caching: Boolean = true,
    val layout: LayoutOptions? = null,
) {
    init {
        pageNumbers?.forEach { page ->
            require(page > 0) { "Page numbers are 1-based; expected > 0 but got $page" }
        }
        if (maxPages != null) {
            require(maxPages > 0) { "maxPages must be > 0 when provided" }
        }
    }

    @PdfDsl
    class Builder {
        private var password: String? = null
        private var pageNumbers: List<Int>? = null
        private var maxPages: Int? = null
        private var caching: Boolean = true
        private var layout: LayoutOptions? = null

        fun password(value: String?) = apply { password = value }

        fun pages(vararg numbers: Int) = apply { pageNumbers = numbers.toList() }

        fun pageNumbers(values: Iterable<Int>?) = apply { pageNumbers = values?.toList() }

        fun maxPages(value: Int?) = apply { maxPages = value }

        fun caching(value: Boolean) = apply { caching = value }

        fun layout(value: LayoutOptions?) = apply { layout = value }

        fun layout(configure: Consumer<LayoutOptions.Builder>) =
            apply {
                val builder = LayoutOptions.builder()
                configure.accept(builder)
                layout = builder.build()
            }

        fun build(): DocumentOptions =
            DocumentOptions(
                password = password,
                pageNumbers = pageNumbers?.toList(),
                maxPages = maxPages,
                caching = caching,
                layout = layout,
            )
    }

    @PdfDsl
    class Dsl {
        var password: String? = null
        private var pageNumbers: List<Int>? = null
        var maxPages: Int? = null
        var caching: Boolean = true
        private var layout: LayoutOptions? = null

        fun pages(vararg numbers: Int) {
            pageNumbers = numbers.toList()
        }

        fun pages(range: IntRange) {
            pageNumbers = range.toList()
        }

        fun pageNumbers(values: Iterable<Int>?) {
            pageNumbers = values?.toList()
        }

        fun layout(value: LayoutOptions?) {
            layout = value
        }

        fun layout(block: LayoutOptions.Builder.() -> Unit) {
            layout = LayoutOptions.build(block)
        }

        internal fun build(): DocumentOptions =
            DocumentOptions(
                password = password,
                pageNumbers = pageNumbers?.toList(),
                maxPages = maxPages,
                caching = caching,
                layout = layout,
            )
    }

    companion object {
        @JvmStatic
        fun builder(): Builder = Builder()

        @JvmSynthetic
        fun build(block: Dsl.() -> Unit): DocumentOptions = Dsl().apply(block).build()

        @JvmSynthetic
        operator fun invoke(block: Dsl.() -> Unit): DocumentOptions = build(block)
    }
}

internal fun DocumentOptions.toNative(): NativeExtractOptions =
    NativeExtractOptions(
        password = password,
        pageNumbers = pageNumbers?.toPageNumbersUInt(),
        maxPages = maxPages?.toPageNumberUInt(),
        caching = caching,
        layoutParams = layout?.toNative(),
    )
