package sa.ingenious.pdf

import sa.ingenious.ffi.LayoutParams as NativeLayoutOptions

@JvmRecord
data class LayoutOptions(
    val lineOverlap: Double = 0.5,
    val charMargin: Double = 2.0,
    val lineMargin: Double = 0.5,
    val wordMargin: Double = 0.1,
    val boxesFlow: Double? = 0.5,
    val detectVertical: Boolean = false,
    val allTexts: Boolean = false,
) {
    init {
        if (boxesFlow != null) {
            require(boxesFlow in -1.0..1.0) {
                "boxesFlow must be null or within [-1.0, 1.0]"
            }
        }
    }

    @PdfDsl
    class Builder {
        @get:JvmSynthetic
        @set:JvmSynthetic
        var lineOverlap: Double = 0.5

        @get:JvmSynthetic
        @set:JvmSynthetic
        var charMargin: Double = 2.0

        @get:JvmSynthetic
        @set:JvmSynthetic
        var lineMargin: Double = 0.5

        @get:JvmSynthetic
        @set:JvmSynthetic
        var wordMargin: Double = 0.1

        @get:JvmSynthetic
        @set:JvmSynthetic
        var boxesFlow: Double? = 0.5

        @get:JvmSynthetic
        @set:JvmSynthetic
        var detectVertical: Boolean = false

        @get:JvmSynthetic
        @set:JvmSynthetic
        var allTexts: Boolean = false

        fun lineOverlap(value: Double) = apply { lineOverlap = value }

        fun charMargin(value: Double) = apply { charMargin = value }

        fun lineMargin(value: Double) = apply { lineMargin = value }

        fun wordMargin(value: Double) = apply { wordMargin = value }

        fun boxesFlow(value: Double?) = apply { boxesFlow = value }

        fun detectVertical(value: Boolean) = apply { detectVertical = value }

        fun allTexts(value: Boolean) = apply { allTexts = value }

        fun build(): LayoutOptions =
            LayoutOptions(
                lineOverlap = lineOverlap,
                charMargin = charMargin,
                lineMargin = lineMargin,
                wordMargin = wordMargin,
                boxesFlow = boxesFlow,
                detectVertical = detectVertical,
                allTexts = allTexts,
            )
    }

    companion object {
        @JvmStatic
        fun builder(): Builder = Builder()

        @JvmSynthetic
        fun build(block: Builder.() -> Unit): LayoutOptions = Builder().apply(block).build()

        @JvmSynthetic
        operator fun invoke(block: Builder.() -> Unit): LayoutOptions = build(block)
    }
}

internal fun LayoutOptions.toNative(): NativeLayoutOptions =
    NativeLayoutOptions(
        lineOverlap = lineOverlap,
        charMargin = charMargin,
        lineMargin = lineMargin,
        wordMargin = wordMargin,
        boxesFlow = boxesFlow,
        detectVertical = detectVertical,
        allTexts = allTexts,
    )
