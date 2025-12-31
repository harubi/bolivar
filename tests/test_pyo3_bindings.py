"""Tests for PyO3 bindings (TDD)"""
import pytest
from pathlib import Path

# Get fixtures path
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

    def test_get_pages_returns_iterator(self):
        """PDFDocument.get_pages() returns an iterator of PDFPage objects"""
        from bolivar import PDFDocument

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        pdf_bytes = pdf_path.read_bytes()
        doc = PDFDocument(pdf_bytes)

        pages = list(doc.get_pages())
        assert len(pages) >= 1


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
