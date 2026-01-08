import asyncio


def build_minimal_pdf_with_pages(page_count: int) -> bytes:
    out = []
    offsets = []

    def push(obj: str) -> None:
        offsets.append(sum(len(part) for part in out))
        out.append(obj)

    out.append("%PDF-1.4\n")
    push("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n")

    kids = " ".join(f"{3 + i} 0 R" for i in range(page_count))
    push(
        f"2 0 obj\n<< /Type /Pages /Kids [{kids}] /Count {page_count} >>\nendobj\n"
    )

    for i in range(page_count):
        page_id = 3 + i
        contents_id = 3 + page_count + i
        push(
            f"{page_id} 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents {contents_id} 0 R >>\nendobj\n"
        )

    for i in range(page_count):
        contents_id = 3 + page_count + i
        push(
            f"{contents_id} 0 obj\n<< /Length 0 >>\nstream\n\nendstream\nendobj\n"
        )

    xref_pos = sum(len(part) for part in out)
    obj_count = len(offsets)
    out.append(f"xref\n0 {obj_count + 1}\n0000000000 65535 f \n")
    for offset in offsets:
        out.append(f"{offset:010} 00000 n \n")
    out.append("trailer\n<< /Size ")
    out.append(str(obj_count + 1))
    out.append(" /Root 1 0 R >>\nstartxref\n")
    out.append(str(xref_pos))
    out.append("\n%%EOF")

    return "".join(out).encode()


def test_pyo3_async_runtimes_poc():
    from bolivar import async_runtime_poc

    async def run_poc():
        return await async_runtime_poc()

    result = asyncio.run(run_poc())
    assert result == 42


def test_extract_pages_async_ordered():
    from bolivar import extract_pages_async

    pdf_data = build_minimal_pdf_with_pages(3)

    async def collect_pages():
        page_ids = []
        async for page in extract_pages_async(pdf_data):
            page_ids.append(page.pageid)
        return page_ids

    page_ids = asyncio.run(collect_pages())
    assert page_ids == [1, 2, 3]
