#!/usr/bin/env kotlin
@file:DependsOn("crates/uniffi/jvm/build/libs/bolivar-1.2.0.jar")
@file:DependsOn("net.java.dev.jna:jna:5.18.1")
@file:DependsOn("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.10.2")
@file:DependsOn("org.jetbrains.kotlinx:kotlinx-coroutines-jdk8:1.10.2")

import sa.ingenious.bolivar.Bolivar
import sa.ingenious.bolivar.BolivarNativeLoader
import sa.ingenious.bolivar.DocumentOptions
import java.io.File
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

Bolivar.open(
    pdfPath.path,
    DocumentOptions {
        pages(1, 2) // 1-based pages
        maxPages = 2
    },
).use { doc ->
    val syncText = doc.extractTables()
    println(syncText)
}

runBlocking {
    val asyncText = Bolivar.extractTextAsync(
        pdfPath.path,
        DocumentOptions {
            pages(1, 2)
            maxPages = 2
        },
    )
    println("async chars=${asyncText.length}")
}
