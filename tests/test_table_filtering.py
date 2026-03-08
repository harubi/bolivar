import os

from tests.test_pdfplumber_patch import _reload_pdfplumber

HERE = os.path.join(os.path.dirname(__file__), "..", "references/pdfplumber/tests")
PDF_PATH = os.path.join(HERE, "pdfs/issue-140-example.pdf")


def test_filtered_page_tables_use_rust(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    from bolivar._bridge_api import _extract_tables_for_compat_page

    with pdfplumber.open(PDF_PATH) as pdf:
        page = pdf.pages[0]
        filtered = page.filter(lambda obj: obj.get("object_type") == "char")
        expected = _extract_tables_for_compat_page(
            filtered.chars,
            filtered.lines,
            filtered.rects,
            filtered.curves,
            (
                tuple(filtered.bbox),
                tuple(filtered.mediabox),
                float(filtered.initial_doctop),
                not getattr(filtered, "is_original", True),
            ),
            table_settings=None,
        )
        got = filtered.extract_tables()
    assert got == expected


def test_text_layout_parity(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)

    with pdfplumber.open(PDF_PATH) as pdf:
        page = pdf.pages[0]
        settings = {"text_layout": True}
        expected = page.extract_table(settings)
        tables = page.extract_tables(settings)
        if tables:
            got = max(tables, key=lambda table: sum(len(row) for row in table))
        else:
            got = None
    assert got == expected
