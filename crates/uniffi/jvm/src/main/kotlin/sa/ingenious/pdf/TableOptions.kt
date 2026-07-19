package sa.ingenious.pdf

import sa.ingenious.ffi.BoundingBox as NativeBoundingBox
import sa.ingenious.ffi.TableOptions as NativeTableOptions

/**
 * Table extraction tuning mirroring the pdfplumber-compatible settings.
 *
 * General tolerances fan into both axes; axis-specific values override them.
 * Crops are pdfplumber-space page regions; [firstPageCrop] wins over [crop]
 * on the first page.
 */
@JvmRecord
data class TableOptions(
    val verticalStrategy: String? = null,
    val horizontalStrategy: String? = null,
    val snapTolerance: Double? = null,
    val snapXTolerance: Double? = null,
    val snapYTolerance: Double? = null,
    val joinTolerance: Double? = null,
    val joinXTolerance: Double? = null,
    val joinYTolerance: Double? = null,
    val intersectionTolerance: Double? = null,
    val intersectionXTolerance: Double? = null,
    val intersectionYTolerance: Double? = null,
    val explicitVerticalLines: List<Double>? = null,
    val explicitHorizontalLines: List<Double>? = null,
    val crop: BoundingBox? = null,
    val firstPageCrop: BoundingBox? = null,
    val maxPages: Int? = null,
) {
    init {
        if (maxPages != null) {
            require(maxPages > 0) { "maxPages must be > 0 when provided" }
        }
    }

    @PdfDsl
    class Builder {
        private var verticalStrategy: String? = null
        private var horizontalStrategy: String? = null
        private var snapTolerance: Double? = null
        private var snapXTolerance: Double? = null
        private var snapYTolerance: Double? = null
        private var joinTolerance: Double? = null
        private var joinXTolerance: Double? = null
        private var joinYTolerance: Double? = null
        private var intersectionTolerance: Double? = null
        private var intersectionXTolerance: Double? = null
        private var intersectionYTolerance: Double? = null
        private var explicitVerticalLines: List<Double>? = null
        private var explicitHorizontalLines: List<Double>? = null
        private var crop: BoundingBox? = null
        private var firstPageCrop: BoundingBox? = null
        private var maxPages: Int? = null

        fun verticalStrategy(value: String?) = apply { verticalStrategy = value }

        fun horizontalStrategy(value: String?) = apply { horizontalStrategy = value }

        fun snapTolerance(value: Double?) = apply { snapTolerance = value }

        fun snapXTolerance(value: Double?) = apply { snapXTolerance = value }

        fun snapYTolerance(value: Double?) = apply { snapYTolerance = value }

        fun joinTolerance(value: Double?) = apply { joinTolerance = value }

        fun joinXTolerance(value: Double?) = apply { joinXTolerance = value }

        fun joinYTolerance(value: Double?) = apply { joinYTolerance = value }

        fun intersectionTolerance(value: Double?) = apply { intersectionTolerance = value }

        fun intersectionXTolerance(value: Double?) = apply { intersectionXTolerance = value }

        fun intersectionYTolerance(value: Double?) = apply { intersectionYTolerance = value }

        fun explicitVerticalLines(values: Iterable<Double>?) =
            apply { explicitVerticalLines = values?.toList() }

        fun explicitHorizontalLines(values: Iterable<Double>?) =
            apply { explicitHorizontalLines = values?.toList() }

        fun crop(value: BoundingBox?) = apply { crop = value }

        fun firstPageCrop(value: BoundingBox?) = apply { firstPageCrop = value }

        fun maxPages(value: Int?) = apply { maxPages = value }

        fun build(): TableOptions =
            TableOptions(
                verticalStrategy = verticalStrategy,
                horizontalStrategy = horizontalStrategy,
                snapTolerance = snapTolerance,
                snapXTolerance = snapXTolerance,
                snapYTolerance = snapYTolerance,
                joinTolerance = joinTolerance,
                joinXTolerance = joinXTolerance,
                joinYTolerance = joinYTolerance,
                intersectionTolerance = intersectionTolerance,
                intersectionXTolerance = intersectionXTolerance,
                intersectionYTolerance = intersectionYTolerance,
                explicitVerticalLines = explicitVerticalLines,
                explicitHorizontalLines = explicitHorizontalLines,
                crop = crop,
                firstPageCrop = firstPageCrop,
                maxPages = maxPages,
            )
    }

    companion object {
        @JvmStatic
        fun builder(): Builder = Builder()

        fun build(block: Builder.() -> Unit): TableOptions = Builder().apply(block).build()
    }
}

private fun BoundingBox.toNative(): NativeBoundingBox =
    NativeBoundingBox(
        x0 = x0,
        y0 = y0,
        x1 = x1,
        y1 = y1,
    )

internal fun TableOptions.toNative(): NativeTableOptions =
    NativeTableOptions(
        verticalStrategy = verticalStrategy,
        horizontalStrategy = horizontalStrategy,
        snapTolerance = snapTolerance,
        snapXTolerance = snapXTolerance,
        snapYTolerance = snapYTolerance,
        joinTolerance = joinTolerance,
        joinXTolerance = joinXTolerance,
        joinYTolerance = joinYTolerance,
        intersectionTolerance = intersectionTolerance,
        intersectionXTolerance = intersectionXTolerance,
        intersectionYTolerance = intersectionYTolerance,
        explicitVerticalLines = explicitVerticalLines,
        explicitHorizontalLines = explicitHorizontalLines,
        crop = crop?.toNative(),
        firstPageCrop = firstPageCrop?.toNative(),
        maxPages = maxPages?.toUInt(),
    )
