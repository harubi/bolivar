package sa.ingenious.bolivar

@DslMarker
annotation class BolivarDsl

fun documentOptions(block: DocumentOptions.Builder.() -> Unit): DocumentOptions = DocumentOptions.build(block)

fun layoutParams(block: LayoutParams.Builder.() -> Unit): LayoutParams = LayoutParams.build(block)
