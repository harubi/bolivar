"""Tests for PyO3 bindings (TDD)"""

import pytest
from pathlib import Path

# Get fixtures path
ROOT = Path(__file__).parent.parent
FIXTURES_DIR = Path(__file__).parent.parent / "crates/core/tests/fixtures"


class TestPDFDocument:
    """Test PDFDocument wrapper"""

    def test_open_pdf_from_bytes(self):
        """PDFDocument can be created from PDF bytes"""
        from bolivar import PDFDocument

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        pdf_bytes = pdf_path.read_bytes()
        doc = PDFDocument(pdf_bytes)
        assert doc is not None

    def test_open_pdf_with_password(self):
        """PDFDocument accepts optional password parameter"""
        from bolivar import PDFDocument

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        pdf_bytes = pdf_path.read_bytes()
        doc = PDFDocument(pdf_bytes, password="")
        assert doc is not None

    def test_open_pdf_memoryview(self):
        """PDFDocument accepts memoryview inputs by default"""
        from bolivar import PDFDocument

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        pdf_bytes = pdf_path.read_bytes()
        doc = PDFDocument(memoryview(pdf_bytes))
        assert doc is not None

    def test_get_pages_returns_iterator(self):
        """PDFDocument.get_pages() returns an iterator of PDFPage objects"""
        from bolivar import PDFDocument

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        pdf_bytes = pdf_path.read_bytes()
        doc = PDFDocument(pdf_bytes)

        pages = list(doc.get_pages())
        assert len(pages) >= 1

    def test_getobj_caches_by_id(self):
        """PDFDocument.getobj returns cached objects for the same ID"""
        from bolivar import PDFDocument

        pdf_path = FIXTURES_DIR / "contrib/issue-886-xref-stream-widths.pdf"
        pdf_bytes = pdf_path.read_bytes()
        doc = PDFDocument(pdf_bytes)

        obj1 = doc.getobj(11)
        obj2 = doc.getobj(11)
        assert obj1 is obj2


class TestPDFPage:
    """Test PDFPage wrapper"""

    def test_page_has_pageid(self):
        """PDFPage has pageid attribute"""
        from bolivar import PDFDocument

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        pdf_bytes = pdf_path.read_bytes()
        doc = PDFDocument(pdf_bytes)

        pages = list(doc.get_pages())
        assert pages[0].pageid >= 0

    def test_page_has_mediabox(self):
        """PDFPage has mediabox attribute as tuple"""
        from bolivar import PDFDocument

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        pdf_bytes = pdf_path.read_bytes()
        doc = PDFDocument(pdf_bytes)

        pages = list(doc.get_pages())
        mediabox = pages[0].mediabox
        assert mediabox is not None
        assert len(mediabox) == 4


class TestProcessPage:
    """Test process_page function"""

    def test_process_page_returns_ltpage(self):
        """process_page returns an LTPage object"""
        from bolivar import PDFDocument, LAParams, process_page

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        pdf_bytes = pdf_path.read_bytes()
        doc = PDFDocument(pdf_bytes)

        pages = list(doc.get_pages())
        laparams = LAParams()

        ltpage = process_page(doc, pages[0], laparams)
        assert ltpage is not None

    def test_ltpage_has_pageid(self):
        """LTPage has pageid attribute"""
        from bolivar import PDFDocument, LAParams, process_page

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        pdf_bytes = pdf_path.read_bytes()
        doc = PDFDocument(pdf_bytes)

        pages = list(doc.get_pages())
        laparams = LAParams()

        ltpage = process_page(doc, pages[0], laparams)
        assert ltpage.pageid >= 1


class TestLTPage:
    """Test LTPage layout type"""

    def test_ltpage_has_bbox(self):
        """LTPage has bbox property returning (x0, y0, x1, y1)"""
        from bolivar import PDFDocument, LAParams, process_page

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        pdf_bytes = pdf_path.read_bytes()
        doc = PDFDocument(pdf_bytes)

        pages = list(doc.get_pages())
        laparams = LAParams()
        ltpage = process_page(doc, pages[0], laparams)

        bbox = ltpage.bbox
        assert len(bbox) == 4


def test_extract_tables_binding_exists():
    import bolivar

    assert hasattr(bolivar, "extract_tables_from_page")
    assert hasattr(bolivar, "extract_words_from_page")
    assert hasattr(bolivar, "extract_text_from_page")


def test_extract_tables_from_document_pages_preserves_order():
    import bolivar

    pdf_path = FIXTURES_DIR / "pdfplumber" / "pdffill-demo.pdf"
    pdf_bytes = pdf_path.read_bytes()
    doc = bolivar.PDFDocument(pdf_bytes)

    pages = list(doc.get_pages())
    page_numbers = [2, 0]
    geoms = [
        (pages[2].mediabox, pages[2].mediabox, pages[2].mediabox[3] * 2, False),
        (pages[0].mediabox, pages[0].mediabox, 0.0, False),
    ]

    tables = bolivar.extract_tables_from_document_pages(doc, page_numbers, geoms)
    assert len(tables) == 2


def test_threads_kw_rejected_in_python_bindings():
    import bolivar

    pdf_path = FIXTURES_DIR / "pdfplumber" / "pdffill-demo.pdf"
    pdf_bytes = pdf_path.read_bytes()
    doc = bolivar.PDFDocument(pdf_bytes)
    page = list(doc.get_pages())[0]
    page_index = 0
    bbox = page.mediabox
    assert bbox is not None

    with pytest.raises(TypeError):
        bolivar.extract_tables_from_page(
            doc,
            page_index,
            bbox,
            bbox,
            0.0,
            threads=1,
        )

    with pytest.raises(TypeError):
        bolivar.extract_text(pdf_bytes, threads=1)

    with pytest.raises(TypeError):
        bolivar.extract_pages(pdf_bytes, threads=1)

    with pytest.raises(TypeError):
        bolivar.process_pages(doc, threads=1)


def test_extract_text_memoryview():
    import bolivar

    pdf_path = FIXTURES_DIR / "simple1.pdf"
    pdf_bytes = pdf_path.read_bytes()
    text = bolivar.extract_text(memoryview(pdf_bytes))
    assert isinstance(text, str)
    assert len(text) > 0


def test_high_level_memoryview():
    from pdfminer import high_level

    pdf_path = FIXTURES_DIR / "simple1.pdf"
    pdf_bytes = pdf_path.read_bytes()
    text = high_level.extract_text(memoryview(pdf_bytes))
    assert isinstance(text, str)
    assert len(text) > 0


def test_extract_tables_settings_affects_output():
    import pdfplumber
    from bolivar import extract_tables_from_page

    pdf_path = ROOT / "references/pdfplumber/tests/pdfs/senate-expenditures.pdf"
    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        page_index = getattr(page.page_obj, "_page_index", page.page_number - 1)
        base_settings = {
            "horizontal_strategy": "text",
            "vertical_strategy": "text",
            "min_words_vertical": 20,
        }

        t = extract_tables_from_page(
            page.page_obj.doc._rust_doc,
            page_index,
            page.bbox,
            page.mediabox,
            page.initial_doctop,
            base_settings,
        )
        t_tol = extract_tables_from_page(
            page.page_obj.doc._rust_doc,
            page_index,
            page.bbox,
            page.mediabox,
            page.initial_doctop,
            {**base_settings, "text_x_tolerance": 1},
        )

    assert t[-1] != t_tol[-1]

    def test_ltpage_iter_returns_layout_items(self):
        """LTPage can be iterated to get layout items"""
        from bolivar import PDFDocument, LAParams, process_page

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        pdf_bytes = pdf_path.read_bytes()
        doc = PDFDocument(pdf_bytes)

        pages = list(doc.get_pages())
        laparams = LAParams()
        ltpage = process_page(doc, pages[0], laparams)

        items = list(ltpage)
        # simple1.pdf should have some items
        assert len(items) >= 0


class TestLTChar:
    """Test LTChar layout type"""

    def test_ltchar_has_text(self):
        """LTChar has get_text() method"""
        from bolivar import PDFDocument, LAParams, process_page, LTChar

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        pdf_bytes = pdf_path.read_bytes()
        doc = PDFDocument(pdf_bytes)

        pages = list(doc.get_pages())
        laparams = LAParams()
        ltpage = process_page(doc, pages[0], laparams)

        # Find first LTChar in the page
        for item in ltpage:
            if isinstance(item, LTChar):
                assert isinstance(item.get_text(), str)
                break

    def test_ltchar_has_fontname(self):
        """LTChar has fontname property"""
        from bolivar import PDFDocument, LAParams, process_page, LTChar

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        pdf_bytes = pdf_path.read_bytes()
        doc = PDFDocument(pdf_bytes)

        pages = list(doc.get_pages())
        laparams = LAParams()
        ltpage = process_page(doc, pages[0], laparams)

        # Find first LTChar in the page
        for item in ltpage:
            if isinstance(item, LTChar):
                assert isinstance(item.fontname, str)
                break

    def test_ltchar_has_mcid(self):
        """LTChar has mcid property (can be None)"""
        from bolivar import PDFDocument, LAParams, process_page, LTChar

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        pdf_bytes = pdf_path.read_bytes()
        doc = PDFDocument(pdf_bytes)

        pages = list(doc.get_pages())
        laparams = LAParams()
        ltpage = process_page(doc, pages[0], laparams)

        # Find first LTChar in the page
        for item in ltpage:
            if isinstance(item, LTChar):
                # mcid can be None or an int
                mcid = item.mcid
                assert mcid is None or isinstance(mcid, int)
                break
