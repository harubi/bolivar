package sa.ingenious.pdf

import kotlin.test.Test
import kotlin.test.assertEquals

class PageTableRowsTest {
    @Test
    fun reconstructsTablesFromFlatBuffers() {
        val page =
            PageTableRows(
                pageNumber = 3,
                cells = listOf("a", null, null, "b"),
                rowOffsets = intArrayOf(0, 2, 2, 4),
                tableOffsets = intArrayOf(0, 2, 3),
            )

        assertEquals(
            listOf(
                listOf(listOf("a", null), emptyList()),
                listOf(listOf(null, "b")),
            ),
            page.toTables(),
        )
    }

    @Test
    fun preservesEmptyTablesAndRows() {
        val page =
            PageTableRows(
                pageNumber = 1,
                cells = emptyList(),
                rowOffsets = intArrayOf(0, 0),
                tableOffsets = intArrayOf(0, 0, 1),
            )

        assertEquals(listOf(emptyList(), listOf(emptyList())), page.toTables())
    }
}
