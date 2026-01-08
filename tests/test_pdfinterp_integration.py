from pathlib import Path

from pdfminer.pdfinterp import PDFResourceManager, PDFPageInterpreter
from pdfminer.pdfpage import PDFPage
from pdfminer.pdfparser import PDFParser
from pdfminer.pdfdocument import PDFDocument
from pdfminer.converter import PDFPageAggregator

FIXTURES = Path(__file__).resolve().parents[1] / "crates/core/tests/fixtures"


def _load_first_page():
    pdf_path = FIXTURES / "simple1.pdf"
    with open(pdf_path, "rb") as fp:
        parser = PDFParser(fp)
        doc = PDFDocument(parser)
    page = next(PDFPage.create_pages(doc))
    return doc, page


def test_resource_manager_get_font_returns_value():
    _, page = _load_first_page()
    rsrc = PDFResourceManager()
    font_dict = page.resources.get("Font") if hasattr(page, "resources") else None
    assert font_dict, "Expected Font resources in simple1.pdf"
    font_spec = next(iter(font_dict.values()))
    font = rsrc.get_font(0, font_spec)
    assert font is not None


def test_interpreter_produces_layout():
    doc, page = _load_first_page()
    rsrc = PDFResourceManager()
    device = PDFPageAggregator(rsrc)
    interp = PDFPageInterpreter(rsrc, device)
    interp.process_page(page)
    assert device.get_result() is not None


def test_interpreter_does_not_cache_pages():
    doc, page = _load_first_page()
    rsrc = PDFResourceManager()
    device = PDFPageAggregator(rsrc)
    interp = PDFPageInterpreter(rsrc, device)
    assert not hasattr(doc, "_layout_cache")
    interp.process_page(page)
    assert not hasattr(doc, "_layout_cache")
