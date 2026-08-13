package sa.ingenious.pdf

import kotlin.test.Test
import kotlin.test.assertEquals

class PageTableRowsTest {
    @Test
    fun exposesFlatBuffers() {
        val page =
            PageTableRows(
                pageNumber = 3,
                cells = listOf("a", null, null, "b"),
                rowOffsets = intArrayOf(0, 2, 2, 4),
                tableOffsets = intArrayOf(0, 2, 3),
            )

        assertEquals(listOf("a", null, null, "b"), page.cells)
        assertEquals(listOf(0, 2, 2, 4), page.rowOffsets.toList())
        assertEquals(listOf(0, 2, 3), page.tableOffsets.toList())
    }

    @Test
    fun preservesEmptyTableAndRowOffsets() {
        val page =
            PageTableRows(
                pageNumber = 1,
                cells = emptyList(),
                rowOffsets = intArrayOf(0, 0),
                tableOffsets = intArrayOf(0, 0, 1),
            )

        assertEquals(emptyList(), page.cells)
        assertEquals(listOf(0, 0), page.rowOffsets.toList())
        assertEquals(listOf(0, 0, 1), page.tableOffsets.toList())
    }
}
