from __future__ import annotations

import asyncio
from pathlib import Path


def test_public_bolivar_table_entrypoints_are_removed() -> None:
    import bolivar

    assert not hasattr(bolivar, "extract_tables_from_document")
    assert not hasattr(bolivar, "extract_tables_stream_from_document")
    assert not hasattr(bolivar, "_extract_tables_core")


def test_pdfplumber_async_table_extraction_still_works() -> None:
    import pdfplumber

    path = Path("references/pdfplumber/tests/pdfs/nics-background-checks-2015-11.pdf")

    async def run() -> None:
        async with pdfplumber.open(path) as pdf:
            async for page in pdf.pages:
                tables = page.extract_tables()
                assert isinstance(tables, list)
                break

    asyncio.run(run())
