import os

from tests.test_pdfplumber_patch import _reload_pdfplumber

HERE = os.path.join(os.path.dirname(__file__), "..", "references/pdfplumber/tests")
PDF_PATH = os.path.join(HERE, "pdfs/issue-140-example.pdf")


def test_filtered_page_tables_use_rust(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    from bolivar._bolivar import _extract_tables_from_page_objects

    with pdfplumber.open(PDF_PATH) as pdf:
        page = pdf.pages[0]
        filtered = page.filter(lambda obj: obj.get("object_type") == "char")
        expected = _extract_tables_from_page_objects(
            filtered.objects,
            filtered.bbox,
            filtered.mediabox,
            filtered.initial_doctop,
            table_settings=None,
            force_crop=not getattr(filtered, "is_original", True),
        )
        got = filtered.extract_tables()
    assert got == expected


def test_text_layout_parity(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    from bolivar import _extract_tables_core

    with pdfplumber.open(PDF_PATH) as pdf:
        page = pdf.pages[0]
        settings = {"text_layout": True}
        expected = page.extract_table(settings)
        page_index = getattr(page.page_obj, "_page_index", page.page_number - 1)
        tables = _extract_tables_core(
            page.page_obj.doc._rust_doc,
            page_index,
            page.bbox,
            page.mediabox,
            page.initial_doctop,
            table_settings=settings,
            force_crop=False,
        )
        if tables:
            got = max(tables, key=lambda table: sum(len(row) for row in table))
        else:
            got = None
    assert got == expected
