import os

from tests.test_pdfplumber_patch import _reload_pdfplumber

HERE = os.path.join(os.path.dirname(__file__), "..", "references/pdfplumber/tests")
PDF_PATH = os.path.join(HERE, "pdfs/issue-140-example.pdf")


def test_filtered_page_tables_use_rust(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    from bolivar import extract_tables_from_page_filtered

    with pdfplumber.open(PDF_PATH) as pdf:
        page = pdf.pages[0]
        filtered = page.filter(lambda obj: obj.get("object_type") == "char")
        expected = extract_tables_from_page_filtered(filtered, table_settings=None)
        got = filtered.extract_tables()
    assert got == expected


def test_text_layout_parity(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    from bolivar import extract_table_from_page

    with pdfplumber.open(PDF_PATH) as pdf:
        page = pdf.pages[0]
        settings = {"text_layout": True}
        expected = page.extract_table(settings)
        page_index = getattr(page.page_obj, "_page_index", page.page_number - 1)
        got = extract_table_from_page(
            page.page_obj.doc._rust_doc,
            page_index,
            page.bbox,
            page.mediabox,
            page.initial_doctop,
            table_settings=settings,
            force_crop=False,
        )
    assert got == expected
