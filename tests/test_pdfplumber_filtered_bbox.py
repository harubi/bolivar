import os
import sys

from tests.test_pdfplumber_patch import _reload_pdfplumber

ROOT = os.path.dirname(os.path.dirname(__file__))
SHIM_PATH = os.path.join(ROOT, "crates", "python", "python")
if SHIM_PATH in sys.path:
    sys.path.remove(SHIM_PATH)
sys.path.insert(0, SHIM_PATH)


def test_filtered_page_list_bbox(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)

    assert (
        getattr(pdfplumber.page.Page.extract_tables, "_bolivar_patched", False) is True
    )

    pdf_path = os.path.join(
        ROOT,
        "references",
        "pdfplumber",
        "tests",
        "pdfs",
        "pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        page.bbox = list(page.bbox)
        filtered = page.filter(lambda obj: True)
        tables = filtered.extract_tables()

    assert tables is not None
