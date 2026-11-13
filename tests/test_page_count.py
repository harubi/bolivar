"""Page enumeration follows the page tree, like pdfminer.six, not /Count."""

from io import BytesIO

import pytest

import pdfplumber
from pdfminer.high_level import extract_pages


def build_pdf(page_count: int, count_entry: str, *, with_pages: bool = True) -> bytes:
    """Build a PDF whose /Pages node carries `count_entry` as its /Count."""
    out: list[str] = []
    offsets: list[int] = []

    def push(obj: str) -> None:
        offsets.append(sum(len(part) for part in out))
        out.append(obj)

    out.append("%PDF-1.4\n")
    push("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n")
    kids = " ".join(f"{3 + i} 0 R" for i in range(page_count)) if with_pages else ""
    push(f"2 0 obj\n<< /Type /Pages /Kids [{kids}] {count_entry} >>\nendobj\n")
    for i in range(page_count):
        page_type = "/Page" if with_pages else "/Other"
        push(
            f"{3 + i} 0 obj\n<< /Type {page_type} /Parent 2 0 R "
            "/MediaBox [0 0 200 200] >>\nendobj\n"
        )

    xref_pos = sum(len(part) for part in out)
    out.append(f"xref\n0 {len(offsets) + 1}\n0000000000 65535 f \n")
    out.extend(f"{offset:010} 00000 n \n" for offset in offsets)
    out.append(f"trailer\n<< /Size {len(offsets) + 1} /Root 1 0 R >>\n")
    out.append(f"startxref\n{xref_pos}\n%%EOF")
    return "".join(out).encode("ascii")


@pytest.mark.parametrize(
    "count_entry",
    ["/Count 2", "", "/Count 7", "/Count 0", "/Count /Two"],
    ids=["declared", "missing", "too-high", "zero", "not-a-number"],
)
def test_pages_follow_page_tree(count_entry: str) -> None:
    data = build_pdf(2, count_entry)

    with pdfplumber.open(BytesIO(data)) as pdf:
        assert len(pdf.pages) == 2
        assert [page.page_number for page in pdf.pages] == [1, 2]

    assert len(list(extract_pages(BytesIO(data)))) == 2


def test_document_without_pages_is_empty() -> None:
    data = build_pdf(1, "/Count 1", with_pages=False)

    with pdfplumber.open(BytesIO(data)) as pdf:
        assert pdf.pages == []

    assert list(extract_pages(BytesIO(data))) == []


def test_rootless_data_without_pages_raises_like_pdfminer() -> None:
    from pdfminer.pdfdocument import PDFDocument
    from pdfminer.pdfparser import PDFParser, PDFSyntaxError

    data = (
        b"%PDF-1.4\n1 0 obj\n<< /Length 11 >>\nstream\nhello world\nendstream\nendobj\n"
    )
    with pytest.raises(PDFSyntaxError, match="No /Root object"):
        PDFDocument(PDFParser(BytesIO(data)))
