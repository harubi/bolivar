package sa.ingenious.pdf

@DslMarker
annotation class PdfDsl

@JvmSynthetic
fun documentOptions(block: DocumentOptions.Dsl.() -> Unit): DocumentOptions =
    DocumentOptions.build(block)

@JvmSynthetic
fun layoutOptions(block: LayoutOptions.Builder.() -> Unit): LayoutOptions =
    LayoutOptions.build(block)
