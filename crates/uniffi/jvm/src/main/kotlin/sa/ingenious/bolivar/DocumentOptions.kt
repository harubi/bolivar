package sa.ingenious.bolivar

import sa.ingenious.bolivar.ffi.ExtractOptions as NativeExtractOptions

data class DocumentOptions(
    val password: String? = null,
    val pageNumbers: List<Int>? = null,
    val maxPages: Int? = null,
    val caching: Boolean = true,
    val layoutParams: LayoutParams? = null,
) {
    init {
        pageNumbers?.forEach { page ->
            require(page > 0) { "Page numbers are 1-based; expected > 0 but got $page" }
        }
        if (maxPages != null) {
            require(maxPages > 0) { "maxPages must be > 0 when provided" }
        }
    }

    @BolivarDsl
    class Builder {
        var password: String? = null
        private var pageNumbers: List<Int>? = null
        var maxPages: Int? = null
        var caching: Boolean = true
        private var layoutParams: LayoutParams? = null

        fun pages(vararg numbers: Int) = apply { pageNumbers = numbers.toList() }

        fun pages(range: IntRange) = apply { pageNumbers = range.toList() }

        fun pageNumbers(values: Iterable<Int>?) = apply { pageNumbers = values?.toList() }

        fun layout(value: LayoutParams?) = apply { layoutParams = value }

        fun layout(block: LayoutParams.Builder.() -> Unit) =
            apply {
                layoutParams = LayoutParams.build(block)
            }

        fun build(): DocumentOptions =
            DocumentOptions(
                password = password,
                pageNumbers = pageNumbers?.toList(),
                maxPages = maxPages,
                caching = caching,
                layoutParams = layoutParams,
            )
    }

    companion object {
        @JvmStatic
        fun builder(): Builder = Builder()

        @JvmStatic
        fun build(block: Builder.() -> Unit): DocumentOptions = Builder().apply(block).build()

        operator fun invoke(block: Builder.() -> Unit): DocumentOptions = build(block)
    }
}

internal fun DocumentOptions.toNative(): NativeExtractOptions =
    NativeExtractOptions(
        password = password,
        pageNumbers = pageNumbers?.toPageNumbersUInt(),
        maxPages = maxPages?.toPageNumberUInt(),
        caching = caching,
        layoutParams = layoutParams?.toNative(),
    )
