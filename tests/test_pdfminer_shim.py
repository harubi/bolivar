"""Tests for pdfminer.six compatibility shim (TDD)

These tests verify that the pdfminer shim provides API compatibility
for pdfplumber and other pdfminer.six consumers.
"""
import pytest
from pathlib import Path
from io import BytesIO

# Get fixtures path
FIXTURES_DIR = Path(__file__).parent.parent / "crates/core/tests/fixtures"
PDFPLUMBER_PDFS = FIXTURES_DIR / "pdfplumber"
NONFREE_PDFS = FIXTURES_DIR / "nonfree"


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

        # Use annotations.pdf which has Producer/Creator metadata
        pdf_path = FIXTURES_DIR / "pdfplumber/annotations.pdf"
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

    def test_page_box_types_optional(self):
        """PDFPage exposes BleedBox, TrimBox, ArtBox as attributes (None if not in PDF)."""
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument
        from pdfminer.pdfpage import PDFPage

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        with open(pdf_path, "rb") as f:
            parser = PDFParser(f)
            doc = PDFDocument(parser)
            for page in PDFPage.create_pages(doc):
                # These are accessible as attributes (None if not in PDF)
                assert hasattr(page, "bleedbox")
                assert hasattr(page, "trimbox")
                assert hasattr(page, "artbox")
                # simple1.pdf doesn't have these boxes, so they should be None
                assert page.bleedbox is None
                assert page.trimbox is None
                assert page.artbox is None
                break

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

    def test_page_has_annots(self):
        """PDFPage.annots returns list of annotation dicts for PDF with annotations."""
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument
        from pdfminer.pdfpage import PDFPage

        # pdffill-demo.pdf has annotations (links)
        pdf_path = FIXTURES_DIR / "pdfplumber/pdffill-demo.pdf"
        with open(pdf_path, "rb") as f:
            parser = PDFParser(f)
            doc = PDFDocument(parser)
            pages = list(PDFPage.create_pages(doc))
            page = pages[0]

            # annots should be a list (not None)
            assert page.annots is not None, "page.annots should not be None"
            assert isinstance(page.annots, list), "page.annots should be a list"

            # This PDF has annotations
            assert len(page.annots) > 0, "Expected annotations in pdffill-demo.pdf"

            # Each annotation should be a dict with Rect
            for annot in page.annots:
                assert isinstance(annot, dict), f"Annotation should be dict, got {type(annot)}"
                assert "Rect" in annot, "Annotation should have Rect field"


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


class TestColorExtraction:
    """Test color extraction from layout items"""

    def test_ltchar_has_color_from_graphicstate(self):
        """LTChar.graphicstate should have actual colors, not defaults."""
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument
        from pdfminer.pdfpage import PDFPage
        from pdfminer.pdfinterp import PDFResourceManager, PDFPageInterpreter
        from pdfminer.converter import PDFPageAggregator
        from pdfminer.layout import LAParams, LTChar

        # pdffill-demo.pdf has colored text
        pdf_path = FIXTURES_DIR / "pdfplumber/pdffill-demo.pdf"
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

                # Find LTChar items and check their colors
                chars_with_color = []
                for item in layout:
                    if isinstance(item, LTChar):
                        if hasattr(item, 'graphicstate') and item.graphicstate:
                            ncolor = item.graphicstate.ncolor
                            # Should NOT be default (0) for colored text
                            if ncolor != 0 and ncolor != (0,):
                                chars_with_color.append(item)

                # pdffill-demo.pdf should have some colored text
                assert len(chars_with_color) > 0, "Expected some chars with non-default colors"
                break

    def test_rgb_color_extraction(self):
        """PDF with RGB text should extract RGB color values correctly."""
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument
        from pdfminer.pdfpage import PDFPage
        from pdfminer.pdfinterp import PDFResourceManager, PDFPageInterpreter
        from pdfminer.converter import PDFPageAggregator
        from pdfminer.layout import LAParams, LTChar

        # nics PDF has red text "November - 2015"
        pdf_path = FIXTURES_DIR / "pdfplumber/nics-background-checks-2015-11.pdf"
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

                # Find red chars (R > 0.9, G < 0.1, B < 0.1)
                red_chars = []
                for item in layout:
                    if isinstance(item, LTChar):
                        if hasattr(item, 'graphicstate') and item.graphicstate:
                            nc = item.graphicstate.ncolor
                            if isinstance(nc, tuple) and len(nc) == 3:
                                r, g, b = nc
                                if r > 0.9 and g < 0.1 and b < 0.1:
                                    red_chars.append(item.get_text())

                # Should find the red "November - 2015" text
                red_text = ''.join(red_chars)
                assert "November" in red_text, f"Expected 'November' in red text, got: {red_text}"
                break


class TestErrorHandling:
    """Test error handling in pdfminer shim"""

    def test_invalid_pdf_raises_value_error(self):
        """Opening invalid PDF should raise ValueError, not panic."""
        from bolivar import PDFDocument
        import pytest

        with pytest.raises(ValueError):
            PDFDocument(b"not a valid pdf", "")


class TestObjectExtraction:
    """Test extraction of graphical objects (rects, lines, curves) - TDD"""

    def test_ltpage_contains_rects(self):
        """LTPage should yield LTRect objects when iterating"""
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument
        from pdfminer.pdfpage import PDFPage
        from pdfminer.pdfinterp import PDFResourceManager, PDFPageInterpreter
        from pdfminer.converter import PDFPageAggregator
        from pdfminer.layout import LAParams, LTRect

        # pdffill-demo.pdf has rectangles
        pdf_path = FIXTURES_DIR / "pdfplumber/pdffill-demo.pdf"
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

                # Find LTRect objects
                rects = [item for item in layout if isinstance(item, LTRect)]
                assert len(rects) > 0, "Should extract LTRect objects from pdffill-demo.pdf"
                break

    def test_ltpage_contains_lines(self):
        """LTPage should yield LTLine objects when iterating"""
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument
        from pdfminer.pdfpage import PDFPage
        from pdfminer.pdfinterp import PDFResourceManager, PDFPageInterpreter
        from pdfminer.converter import PDFPageAggregator
        from pdfminer.layout import LAParams, LTLine

        # nics PDF has lines (table borders, etc.)
        pdf_path = FIXTURES_DIR / "pdfplumber/nics-background-checks-2015-11.pdf"
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

                # Find LTLine objects
                lines = [item for item in layout if isinstance(item, LTLine)]
                assert len(lines) > 0, "Should extract LTLine objects from nics PDF"
                break

    def test_ltcurve_class_available(self):
        """LTCurve class should be importable and usable"""
        from pdfminer.layout import LTCurve

        # Create a curve manually to verify the class works
        curve = LTCurve(
            linewidth=1.0,
            pts=[(0, 0), (50, 100), (100, 0)],
            stroke=True,
            fill=False,
        )
        assert curve.pts == [(0, 0), (50, 100), (100, 0)]
        assert curve.linewidth == 1.0
        assert curve.stroke is True
        assert curve.fill is False
        # Verify bbox is computed from points
        assert curve.x0 == 0
        assert curve.y0 == 0
        assert curve.x1 == 100
        assert curve.y1 == 100


class TestLTAnno:
    """Test LTAnno (virtual annotations) for char indexing compatibility"""

    def test_ltanno_class_available(self):
        """LTAnno class should be importable"""
        from pdfminer.layout import LTAnno

        anno = LTAnno(" ")
        assert anno.get_text() == " "

    def test_layout_includes_ltanno(self):
        """LTPage iteration should include LTAnno objects for spaces"""
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument
        from pdfminer.pdfpage import PDFPage
        from pdfminer.pdfinterp import PDFResourceManager, PDFPageInterpreter
        from pdfminer.converter import PDFPageAggregator
        from pdfminer.layout import LAParams, LTAnno

        pdf_path = FIXTURES_DIR / "pdfplumber/nics-background-checks-2015-11.pdf"
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

                # Find LTAnno objects
                annos = [item for item in layout if isinstance(item, LTAnno)]
                assert len(annos) > 0, "Should include LTAnno objects for spaces"
                break


class TestPdfplumberParity:
    """Test exact parity with pdfminer.six character ordering.

    These tests verify that character indices match pdfminer.six exactly,
    which is required for pdfplumber compatibility.
    """

    def test_char_3358_is_red_n(self):
        """chars[3358] should be a red 'N' from 'November'.

        This is the specific test that motivated the exact grouping algorithm.
        pdfplumber accesses chars by index, so ordering must match pdfminer.six.
        """
        try:
            import pdfplumber
        except ImportError:
            pytest.skip("pdfplumber not installed")

        pdf_path = FIXTURES_DIR / "pdfplumber/nics-background-checks-2015-11.pdf"
        if not pdf_path.exists():
            pytest.skip(f"Test fixture not found: {pdf_path}")

        with pdfplumber.open(pdf_path) as pdf:
            page = pdf.pages[0]
            chars = page.chars

            # Verify we have enough characters
            assert len(chars) > 3358, f"Expected > 3358 chars, got {len(chars)}"

            char = chars[3358]

            # The character should be "N" (from "November")
            assert char["text"] == "N", f"Expected 'N', got {char['text']!r}"

            # The character should have red non-stroking color
            color = char.get("non_stroking_color")
            assert color is not None, "Expected non_stroking_color"

            # Red color: R > 0.9, G < 0.1, B < 0.1
            if isinstance(color, tuple) and len(color) == 3:
                r, g, b = color
                assert r > 0.9 and g < 0.1 and b < 0.1, f"Expected red color, got RGB{color}"


class TestPdfplumberLayoutParity:
    def test_layout_tree_has_textboxes_and_lines(self):
        try:
            import pdfplumber
        except ImportError:
            pytest.skip("pdfplumber not installed")

        pdf_path = PDFPLUMBER_PDFS / "issue-192-example.pdf"
        if not pdf_path.exists():
            pytest.skip(f"Test fixture not found: {pdf_path}")

        with pdfplumber.open(pdf_path, laparams={"detect_vertical": True}) as pdf:
            page = pdf.pages[0]
            assert len(page.textlinehorizontals) > 0
            assert len(page.textboxhorizontals) > 0

    def test_layout_tree_has_images(self):
        try:
            import pdfplumber
        except ImportError:
            pytest.skip("pdfplumber not installed")

        pdf_path = NONFREE_PDFS / "dmca.pdf"
        if not pdf_path.exists():
            pytest.skip(f"Test fixture not found: {pdf_path}")

        with pdfplumber.open(pdf_path) as pdf:
            page = pdf.pages[0]
            assert len(page.images) > 0

    def test_char_matrix_present(self):
        try:
            import pdfplumber
        except ImportError:
            pytest.skip("pdfplumber not installed")

        pdf_path = PDFPLUMBER_PDFS / "pdffill-demo.pdf"
        if not pdf_path.exists():
            pytest.skip(f"Test fixture not found: {pdf_path}")

        with pdfplumber.open(pdf_path) as pdf:
            page = pdf.pages[3]
            assert page.chars[0]["matrix"] is not None

    def test_mcid_present(self):
        try:
            import pdfplumber
        except ImportError:
            pytest.skip("pdfplumber not installed")

        pdf_path = PDFPLUMBER_PDFS / "mcid_example.pdf"
        if not pdf_path.exists():
            pytest.skip(f"Test fixture not found: {pdf_path}")

        with pdfplumber.open(pdf_path) as pdf:
            page = pdf.pages[0]
            mcids = [c.get("mcid") for c in page.chars if "mcid" in c]
            assert any(m is not None for m in mcids)

    def test_doc_xrefs_info_ref(self):
        try:
            import pdfplumber
        except ImportError:
            pytest.skip("pdfplumber not installed")

        from pdfminer.pdfparser import PDFObjRef

        pdf_path = PDFPLUMBER_PDFS / "pdffill-demo.pdf"
        if not pdf_path.exists():
            pytest.skip(f"Test fixture not found: {pdf_path}")

        with pdfplumber.open(pdf_path) as pdf:
            info = pdf.doc.xrefs[0].trailer["Info"]
            assert isinstance(info, PDFObjRef)
