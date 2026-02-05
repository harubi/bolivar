#!/usr/bin/env kotlin
@file:DependsOn("crates/uniffi/jvm/build/libs/bolivar-uniffi-jvm.jar")
@file:DependsOn("net.java.dev.jna:jna:5.18.1")
@file:DependsOn("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.10.2")
@file:DependsOn("org.jetbrains.kotlinx:kotlinx-coroutines-jdk8:1.10.2")

import io.bolivar.jvm.Bolivar
import io.bolivar.jvm.BolivarAsync
import io.bolivar.jvm.BolivarNativeLoader
import java.io.File
import kotlinx.coroutines.future.await
import kotlinx.coroutines.runBlocking

val libName = System.mapLibraryName("bolivar_uniffi")
val libPath = File("target/release/$libName").absoluteFile
require(libPath.exists()) {
    "Native library not found at ${libPath.path}. Build it first with `cargo make build-uniffi`."
}
BolivarNativeLoader.configureLibraryOverride(libPath.path)
// Overrides must be absolute paths for JNA/UniFFI loaders.

val pdfPath = if (args.isNotEmpty()) {
    File(args[0]).absoluteFile
} else {
    File("references/pdfplumber/tests/pdfs/table-curves-example.pdf").absoluteFile
}
require(pdfPath.exists()) {
    "PDF not found at ${pdfPath.path}. Pass a PDF path as the first argument."
}

val syncText = Bolivar.extractTextFromPathWithPageRange(
    pdfPath.path,
    null,
    listOf(1u, 2u), // 1-based pages
    2u
)
println(syncText.take(200))

runBlocking {
    val asyncText = BolivarAsync.extractTextFromPathAsync(pdfPath.path, null).await()
    println("async chars=${asyncText.length}")
}
