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
    fun extractTextFromBytesWithPageRange(
        pdfData: ByteArray,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): String {
        return io.bolivar.`extractTextFromBytesWithPageRange`(pdfData, password, pageNumbers, maxPages)
    }

    @JvmStatic
    fun extractTextFromPath(path: String, password: String?): String {
        return io.bolivar.`extractTextFromPath`(path, password)
    }

    @JvmStatic
    fun extractTextFromPathWithPageRange(
        path: String,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): String {
        return io.bolivar.`extractTextFromPathWithPageRange`(path, password, pageNumbers, maxPages)
    }

    @JvmStatic
    fun extractPageSummariesFromBytes(pdfData: ByteArray, password: String?): List<PageSummary> {
        return io.bolivar.`extractPageSummariesFromBytes`(pdfData, password)
    }

    @JvmStatic
    fun extractPageSummariesFromBytesWithPageRange(
        pdfData: ByteArray,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): List<PageSummary> {
        return io.bolivar.`extractPageSummariesFromBytesWithPageRange`(pdfData, password, pageNumbers, maxPages)
    }

    @JvmStatic
    fun extractPageSummariesFromPath(path: String, password: String?): List<PageSummary> {
        return io.bolivar.`extractPageSummariesFromPath`(path, password)
    }

    @JvmStatic
    fun extractPageSummariesFromPathWithPageRange(
        path: String,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): List<PageSummary> {
        return io.bolivar.`extractPageSummariesFromPathWithPageRange`(path, password, pageNumbers, maxPages)
    }

    @JvmStatic
    fun extractLayoutPagesFromBytes(pdfData: ByteArray, password: String?): List<LayoutPage> {
        return io.bolivar.`extractLayoutPagesFromBytes`(pdfData, password)
    }

    @JvmStatic
    fun extractLayoutPagesFromBytesWithPageRange(
        pdfData: ByteArray,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): List<LayoutPage> {
        return io.bolivar.`extractLayoutPagesFromBytesWithPageRange`(pdfData, password, pageNumbers, maxPages)
    }

    @JvmStatic
    fun extractLayoutPagesFromPath(path: String, password: String?): List<LayoutPage> {
        return io.bolivar.`extractLayoutPagesFromPath`(path, password)
    }

    @JvmStatic
    fun extractLayoutPagesFromPathWithPageRange(
        path: String,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): List<LayoutPage> {
        return io.bolivar.`extractLayoutPagesFromPathWithPageRange`(path, password, pageNumbers, maxPages)
    }

    @JvmStatic
    fun extractTablesFromBytes(pdfData: ByteArray, password: String?): List<Table> {
        return io.bolivar.`extractTablesFromBytes`(pdfData, password)
    }

    @JvmStatic
    fun extractTablesFromBytesWithPageRange(
        pdfData: ByteArray,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): List<Table> {
        return io.bolivar.`extractTablesFromBytesWithPageRange`(pdfData, password, pageNumbers, maxPages)
    }

    @JvmStatic
    fun extractTablesFromPath(path: String, password: String?): List<Table> {
        return io.bolivar.`extractTablesFromPath`(path, password)
    }

    @JvmStatic
    fun extractTablesFromPathWithPageRange(
        path: String,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): List<Table> {
        return io.bolivar.`extractTablesFromPathWithPageRange`(path, password, pageNumbers, maxPages)
    }
}
