package sa.ingenious.bolivar

import sa.ingenious.bolivar.LayoutPage
import sa.ingenious.bolivar.PageSummary
import sa.ingenious.bolivar.Table

object Bolivar {
    @JvmStatic
    fun extractTextFromBytes(pdfData: ByteArray, password: String?): String {
        return sa.ingenious.bolivar.extractTextFromBytes(pdfData, password)
    }

    @JvmStatic
    fun extractTextFromBytesWithPageRange(
        pdfData: ByteArray,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): String {
        return sa.ingenious.bolivar.extractTextFromBytesWithPageRange(pdfData, password, pageNumbers, maxPages)
    }

    @JvmStatic
    fun extractTextFromPath(path: String, password: String?): String {
        return sa.ingenious.bolivar.extractTextFromPath(path, password)
    }

    @JvmStatic
    fun extractTextFromPathWithPageRange(
        path: String,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): String {
        return sa.ingenious.bolivar.extractTextFromPathWithPageRange(path, password, pageNumbers, maxPages)
    }

    @JvmStatic
    fun extractPageSummariesFromBytes(pdfData: ByteArray, password: String?): List<PageSummary> {
        return sa.ingenious.bolivar.extractPageSummariesFromBytes(pdfData, password)
    }

    @JvmStatic
    fun extractPageSummariesFromBytesWithPageRange(
        pdfData: ByteArray,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): List<PageSummary> {
        return sa.ingenious.bolivar.extractPageSummariesFromBytesWithPageRange(pdfData, password, pageNumbers, maxPages)
    }

    @JvmStatic
    fun extractPageSummariesFromPath(path: String, password: String?): List<PageSummary> {
        return sa.ingenious.bolivar.extractPageSummariesFromPath(path, password)
    }

    @JvmStatic
    fun extractPageSummariesFromPathWithPageRange(
        path: String,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): List<PageSummary> {
        return sa.ingenious.bolivar.extractPageSummariesFromPathWithPageRange(path, password, pageNumbers, maxPages)
    }

    @JvmStatic
    fun extractLayoutPagesFromBytes(pdfData: ByteArray, password: String?): List<LayoutPage> {
        return sa.ingenious.bolivar.extractLayoutPagesFromBytes(pdfData, password)
    }

    @JvmStatic
    fun extractLayoutPagesFromBytesWithPageRange(
        pdfData: ByteArray,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): List<LayoutPage> {
        return sa.ingenious.bolivar.extractLayoutPagesFromBytesWithPageRange(pdfData, password, pageNumbers, maxPages)
    }

    @JvmStatic
    fun extractLayoutPagesFromPath(path: String, password: String?): List<LayoutPage> {
        return sa.ingenious.bolivar.extractLayoutPagesFromPath(path, password)
    }

    @JvmStatic
    fun extractLayoutPagesFromPathWithPageRange(
        path: String,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): List<LayoutPage> {
        return sa.ingenious.bolivar.extractLayoutPagesFromPathWithPageRange(path, password, pageNumbers, maxPages)
    }

    @JvmStatic
    fun extractTablesFromBytes(pdfData: ByteArray, password: String?): List<Table> {
        return sa.ingenious.bolivar.extractTablesFromBytes(pdfData, password)
    }

    @JvmStatic
    fun extractTablesFromBytesWithPageRange(
        pdfData: ByteArray,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): List<Table> {
        return sa.ingenious.bolivar.extractTablesFromBytesWithPageRange(pdfData, password, pageNumbers, maxPages)
    }

    @JvmStatic
    fun extractTablesFromPath(path: String, password: String?): List<Table> {
        return sa.ingenious.bolivar.extractTablesFromPath(path, password)
    }

    @JvmStatic
    fun extractTablesFromPathWithPageRange(
        path: String,
        password: String?,
        pageNumbers: List<UInt>?,
        maxPages: UInt?,
    ): List<Table> {
        return sa.ingenious.bolivar.extractTablesFromPathWithPageRange(path, password, pageNumbers, maxPages)
    }
}
