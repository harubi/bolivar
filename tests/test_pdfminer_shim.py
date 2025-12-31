"""Tests for pdfminer.six compatibility shim (TDD)

These tests verify that the pdfminer shim provides API compatibility
for pdfplumber and other pdfminer.six consumers.
"""
import pytest
from pathlib import Path
from io import BytesIO

# Get fixtures path
FIXTURES_DIR = Path(__file__).parent.parent / "crates/core/tests/fixtures"


class TestPDFParser:
    """Test pdfminer.pdfparser.PDFParser shim"""

    def test_parser_from_stream(self):
        """PDFParser can be created from file stream"""
        from pdfminer.pdfparser import PDFParser

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        with open(pdf_path, "rb") as f:
            parser = PDFParser(f)
            assert parser is not None

    def test_parser_from_bytes_io(self):
        """PDFParser can be created from BytesIO"""
        from pdfminer.pdfparser import PDFParser

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        pdf_bytes = pdf_path.read_bytes()
        stream = BytesIO(pdf_bytes)
        parser = PDFParser(stream)
        assert parser is not None


class TestPDFDocument:
    """Test pdfminer.pdfdocument.PDFDocument shim"""

    def test_document_from_parser(self):
        """PDFDocument can be created from PDFParser"""
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        with open(pdf_path, "rb") as f:
            parser = PDFParser(f)
            doc = PDFDocument(parser)
            assert doc is not None

    def test_document_with_password(self):
        """PDFDocument accepts password parameter"""
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        with open(pdf_path, "rb") as f:
            parser = PDFParser(f)
            doc = PDFDocument(parser, password="")
            assert doc is not None

    def test_document_has_info(self):
        """PDFDocument has info attribute (list of dicts)"""
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        with open(pdf_path, "rb") as f:
            parser = PDFParser(f)
            doc = PDFDocument(parser)
            # info should be a list (may be empty)
            assert isinstance(doc.info, list)

    def test_document_metadata_has_content(self):
        """PDFDocument.info should contain metadata keys if present in PDF."""
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        with open(pdf_path, "rb") as f:
            parser = PDFParser(f)
            doc = PDFDocument(parser)
            # info is a list of info dicts (one per trailer)
            assert len(doc.info) > 0, "Expected at least one info dict"
            # At least one should have metadata
            all_keys = set()
            for info_dict in doc.info:
                all_keys.update(info_dict.keys())
            assert len(all_keys) > 0, "Expected some metadata keys"


class TestPDFPage:
    """Test pdfminer.pdfpage.PDFPage shim"""

    def test_create_pages_iterator(self):
        """PDFPage.create_pages returns iterator over pages"""
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument
        from pdfminer.pdfpage import PDFPage

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        with open(pdf_path, "rb") as f:
            parser = PDFParser(f)
            doc = PDFDocument(parser)
            pages = list(PDFPage.create_pages(doc))
            assert len(pages) >= 1

    def test_page_has_pageid(self):
        """PDFPage has pageid attribute"""
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument
        from pdfminer.pdfpage import PDFPage

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        with open(pdf_path, "rb") as f:
            parser = PDFParser(f)
            doc = PDFDocument(parser)
            pages = list(PDFPage.create_pages(doc))
            assert hasattr(pages[0], "pageid")

    def test_page_has_mediabox(self):
        """PDFPage has mediabox attribute"""
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument
        from pdfminer.pdfpage import PDFPage

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        with open(pdf_path, "rb") as f:
            parser = PDFParser(f)
            doc = PDFDocument(parser)
            pages = list(PDFPage.create_pages(doc))
            assert hasattr(pages[0], "mediabox")


class TestPDFResourceManager:
    """Test pdfminer.pdfinterp.PDFResourceManager shim"""

    def test_resource_manager_creation(self):
        """PDFResourceManager can be created"""
        from pdfminer.pdfinterp import PDFResourceManager

        rsrcmgr = PDFResourceManager()
        assert rsrcmgr is not None

    def test_resource_manager_with_caching(self):
        """PDFResourceManager accepts caching parameter"""
        from pdfminer.pdfinterp import PDFResourceManager

        rsrcmgr = PDFResourceManager(caching=True)
        assert rsrcmgr is not None


class TestLAParams:
    """Test pdfminer.layout.LAParams shim"""

    def test_laparams_creation(self):
        """LAParams can be created with defaults"""
        from pdfminer.layout import LAParams

        laparams = LAParams()
        assert laparams is not None

    def test_laparams_with_kwargs(self):
        """LAParams accepts keyword arguments"""
        from pdfminer.layout import LAParams

        laparams = LAParams(
            line_overlap=0.5,
            char_margin=2.0,
            word_margin=0.1,
            boxes_flow=0.5,
        )
        assert laparams.char_margin == 2.0


class TestPDFPageInterpreter:
    """Test pdfminer.pdfinterp.PDFPageInterpreter shim"""

    def test_interpreter_creation(self):
        """PDFPageInterpreter can be created with rsrcmgr and device"""
        from pdfminer.pdfinterp import PDFResourceManager, PDFPageInterpreter
        from pdfminer.converter import PDFPageAggregator
        from pdfminer.layout import LAParams

        rsrcmgr = PDFResourceManager()
        laparams = LAParams()
        device = PDFPageAggregator(rsrcmgr, laparams=laparams)
        interpreter = PDFPageInterpreter(rsrcmgr, device)
        assert interpreter is not None

    def test_interpreter_process_page(self):
        """PDFPageInterpreter.process_page works"""
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument
        from pdfminer.pdfpage import PDFPage
        from pdfminer.pdfinterp import PDFResourceManager, PDFPageInterpreter
        from pdfminer.converter import PDFPageAggregator
        from pdfminer.layout import LAParams

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        with open(pdf_path, "rb") as f:
            parser = PDFParser(f)
            doc = PDFDocument(parser)

            rsrcmgr = PDFResourceManager()
            laparams = LAParams()
            device = PDFPageAggregator(rsrcmgr, laparams=laparams)
            interpreter = PDFPageInterpreter(rsrcmgr, device)

            for page in PDFPage.create_pages(doc):
                interpreter.process_page(page)
                layout = device.get_result()
                assert layout is not None
                break  # Just test first page


class TestPDFPageAggregator:
    """Test pdfminer.converter.PDFPageAggregator shim"""

    def test_aggregator_creation(self):
        """PDFPageAggregator can be created"""
        from pdfminer.pdfinterp import PDFResourceManager
        from pdfminer.converter import PDFPageAggregator
        from pdfminer.layout import LAParams

        rsrcmgr = PDFResourceManager()
        laparams = LAParams()
        device = PDFPageAggregator(rsrcmgr, laparams=laparams)
        assert device is not None

    def test_aggregator_get_result(self):
        """PDFPageAggregator.get_result returns LTPage"""
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument
        from pdfminer.pdfpage import PDFPage
        from pdfminer.pdfinterp import PDFResourceManager, PDFPageInterpreter
        from pdfminer.converter import PDFPageAggregator
        from pdfminer.layout import LAParams, LTPage

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        with open(pdf_path, "rb") as f:
            parser = PDFParser(f)
            doc = PDFDocument(parser)

            rsrcmgr = PDFResourceManager()
            laparams = LAParams()
            device = PDFPageAggregator(rsrcmgr, laparams=laparams)
            interpreter = PDFPageInterpreter(rsrcmgr, device)

            for page in PDFPage.create_pages(doc):
                interpreter.process_page(page)
                layout = device.get_result()
                assert isinstance(layout, LTPage)
                break
