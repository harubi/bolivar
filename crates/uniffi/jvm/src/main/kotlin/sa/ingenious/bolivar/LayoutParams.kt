package sa.ingenious.bolivar

import sa.ingenious.bolivar.ffi.LayoutParams as NativeLayoutParams

data class LayoutParams(
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

    @BolivarDsl
    class Builder {
        var lineOverlap: Double = 0.5
        var charMargin: Double = 2.0
        var lineMargin: Double = 0.5
        var wordMargin: Double = 0.1
        var boxesFlow: Double? = 0.5
        var detectVertical: Boolean = false
        var allTexts: Boolean = false

        fun build(): LayoutParams =
            LayoutParams(
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

        @JvmStatic
        fun build(block: Builder.() -> Unit): LayoutParams = Builder().apply(block).build()

        operator fun invoke(block: Builder.() -> Unit): LayoutParams = build(block)
    }
}

internal fun LayoutParams.toNative(): NativeLayoutParams =
    NativeLayoutParams(
        lineOverlap = lineOverlap,
        charMargin = charMargin,
        lineMargin = lineMargin,
        wordMargin = wordMargin,
        boxesFlow = boxesFlow,
        detectVertical = detectVertical,
        allTexts = allTexts,
    )
