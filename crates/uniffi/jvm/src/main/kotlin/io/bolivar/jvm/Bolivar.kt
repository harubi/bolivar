package io.bolivar.jvm

import io.bolivar.LayoutPage
import io.bolivar.PageSummary
import io.bolivar.Table

object Bolivar {
    @JvmStatic
    fun extractTextFromBytes(pdfData: ByteArray, password: String?): String {
        return io.bolivar.`extractTextFromBytes`(pdfData, password)
    }

    @JvmStatic
    fun extractTextFromPath(path: String, password: String?): String {
        return io.bolivar.`extractTextFromPath`(path, password)
    }

    @JvmStatic
    fun extractPageSummariesFromBytes(pdfData: ByteArray, password: String?): List<PageSummary> {
        return io.bolivar.`extractPageSummariesFromBytes`(pdfData, password)
    }

    @JvmStatic
    fun extractPageSummariesFromPath(path: String, password: String?): List<PageSummary> {
        return io.bolivar.`extractPageSummariesFromPath`(path, password)
    }

    @JvmStatic
    fun extractLayoutPagesFromBytes(pdfData: ByteArray, password: String?): List<LayoutPage> {
        return io.bolivar.`extractLayoutPagesFromBytes`(pdfData, password)
    }

    @JvmStatic
    fun extractLayoutPagesFromPath(path: String, password: String?): List<LayoutPage> {
        return io.bolivar.`extractLayoutPagesFromPath`(path, password)
    }

    @JvmStatic
    fun extractTablesFromBytes(pdfData: ByteArray, password: String?): List<Table> {
        return io.bolivar.`extractTablesFromBytes`(pdfData, password)
    }

    @JvmStatic
    fun extractTablesFromPath(path: String, password: String?): List<Table> {
        return io.bolivar.`extractTablesFromPath`(path, password)
    }
}
