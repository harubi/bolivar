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


def build_minimal_pdf_with_pages(page_count: int) -> bytes:
    out = []
    offsets = []
    page_ids = list(range(1, page_count + 1))
    contents_start = page_count + 1
    catalog_id = (2 * page_count) + 1
    pages_id = catalog_id + 1

    def push(obj: str) -> None:
        offsets.append(sum(len(part) for part in out))
        out.append(obj)

    out.append("%PDF-1.4\n")

    for i in range(page_count):
        page_id = page_ids[i]
        contents_id = contents_start + i
        push(
            f"{page_id} 0 obj\n<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 200 200] /Contents {contents_id} 0 R >>\nendobj\n"
        )

    for i in range(page_count):
        contents_id = contents_start + i
        push(f"{contents_id} 0 obj\n<< /Length 0 >>\nstream\n\nendstream\nendobj\n")

    push(f"{catalog_id} 0 obj\n<< /Type /Catalog /Pages {pages_id} 0 R >>\nendobj\n")

    kids = " ".join(f"{page_id} 0 R" for page_id in page_ids)
    push(
        f"{pages_id} 0 obj\n<< /Type /Pages /Kids [{kids}] /Count {page_count} >>\nendobj\n"
    )

    xref_pos = sum(len(part) for part in out)
    obj_count = len(offsets)
    out.append(f"xref\n0 {obj_count + 1}\n0000000000 65535 f \n")
    for offset in offsets:
        out.append(f"{offset:010} 00000 n \n")
    out.append("trailer\n<< /Size ")
    out.append(str(obj_count + 1))
    out.append(f" /Root {catalog_id} 0 R >>\nstartxref\n")
    out.append(str(xref_pos))
    out.append("\n%%EOF")

    return "".join(out).encode()


def break_startxref(pdf_bytes: bytes) -> bytes:
    marker = b"startxref\n"
    start = pdf_bytes.rfind(marker)
    assert start != -1
    line_start = start + len(marker)
    line_end = pdf_bytes.find(b"\n", line_start)
    assert line_end != -1
    return pdf_bytes[:line_start] + b"999999" + pdf_bytes[line_end:]


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

    def test_document_pages_are_lazy(self):
        """PDFDocument should not precompute pages on init."""
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        with open(pdf_path, "rb") as f:
            parser = PDFParser(f)
            doc = PDFDocument(parser)
            assert getattr(doc, "_rust_pages", None) is None

    def test_document_page_mediaboxes_and_count(self):
        """PDFDocument exposes page_count and page_mediaboxes."""
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument

        pdf_path = FIXTURES_DIR / "pdfplumber" / "pdffill-demo.pdf"
        with open(pdf_path, "rb") as f:
            doc = PDFDocument(PDFParser(f))
            boxes = doc.page_mediaboxes()
            assert isinstance(boxes, list)
            assert len(boxes) == doc.page_count()
            assert len(boxes[0]) == 4

    def test_getobj_preserves_refs(self):
        """PDFDocument.getobj should return PDFObjRef values."""
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument
        from pdfminer.pdftypes import PDFObjRef

        pdf_path = FIXTURES_DIR / "contrib/issue-886-xref-stream-widths.pdf"
        with open(pdf_path, "rb") as f:
            parser = PDFParser(f)
            doc = PDFDocument(parser)
            obj = doc.getobj(11)
            assert isinstance(obj["DescendantFonts"][0], PDFObjRef)

    def test_document_exposes_permission_flags_on_unencrypted_pdf(self):
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        with open(pdf_path, "rb") as f:
            doc = PDFDocument(PDFParser(f))
            assert doc.is_printable is True
            assert doc.is_modifiable is True
            assert doc.is_extractable is True

    def test_document_exposes_permission_flags_on_encrypted_pdf(self):
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument

        pdf_path = FIXTURES_DIR / "encryption/rc4-40.pdf"
        with open(pdf_path, "rb") as f:
            doc = PDFDocument(PDFParser(f), password="foo")
            assert doc.is_printable is True
            assert doc.is_modifiable is True
            assert doc.is_extractable is True

    def test_pdfdocument_fallback_controls_xref_recovery(self):
        from pdfminer.pdfdocument import PDFDocument
        from pdfminer.pdfpage import PDFPage
        from pdfminer.pdfparser import PDFParser, PDFSyntaxError

        broken_pdf = break_startxref(build_minimal_pdf_with_pages(1))

        recovered = PDFDocument(PDFParser(BytesIO(broken_pdf)), fallback=True)
        assert len(list(PDFPage.create_pages(recovered))) == 1

        with pytest.raises(PDFSyntaxError, match="No /Root object"):
            PDFDocument(PDFParser(BytesIO(broken_pdf)), fallback=False)


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

    def test_create_pages_signature_matches_upstream(self):
        from inspect import signature
        from pdfminer.pdfpage import PDFPage

        params = signature(PDFPage.create_pages).parameters

        assert list(params) == ["document"]

    def test_create_pages_rejects_legacy_compat_keywords(self):
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument
        from pdfminer.pdfpage import PDFPage

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        with open(pdf_path, "rb") as f:
            doc = PDFDocument(PDFParser(f))
            with pytest.raises(TypeError, match="caching"):
                list(PDFPage.create_pages(doc, caching=True))

    def test_get_pages_signature_uses_upstream_keywords(self):
        from inspect import signature
        from pdfminer.pdfpage import PDFPage

        params = signature(PDFPage.get_pages).parameters

        assert "pagenos" in params
        assert "page_numbers" not in params
        assert params["check_extractable"].default is False

    def test_get_pages_applies_maxpages_after_pagenos_filter(self):
        from io import BytesIO
        from pdfminer.pdfpage import PDFPage

        pdf_bytes = build_minimal_pdf_with_pages(5)
        pages = list(PDFPage.get_pages(BytesIO(pdf_bytes), pagenos={3}, maxpages=1))

        assert len(pages) == 1
        assert pages[0].pageid == 4

    def test_get_pages_rejects_legacy_page_numbers_keyword(self):
        from io import BytesIO
        from pdfminer.pdfpage import PDFPage

        pdf_bytes = build_minimal_pdf_with_pages(5)

        with pytest.raises(TypeError, match="page_numbers"):
            list(PDFPage.get_pages(BytesIO(pdf_bytes), page_numbers={3}, maxpages=1))

    def test_get_pages_treats_empty_pagenos_as_no_filter(self):
        from io import BytesIO
        from pdfminer.pdfpage import PDFPage

        pdf_bytes = build_minimal_pdf_with_pages(5)
        pages = list(PDFPage.get_pages(BytesIO(pdf_bytes), pagenos=set(), maxpages=1))

        assert len(pages) == 1
        assert pages[0].pageid == 1

    def test_get_pages_raises_when_document_is_not_extractable(self, monkeypatch):
        from io import BytesIO
        from pdfminer.pdfexceptions import PDFTextExtractionNotAllowed
        from pdfminer.pdfpage import PDFPage

        class StubParser:
            def __init__(self, fp):
                self.fp = fp

        class StubDocument:
            def __init__(self, parser, password=b"", caching=True):
                self.parser = parser
                self.password = password
                self.caching = caching
                self.is_extractable = False

        monkeypatch.setattr("pdfminer.pdfparser.PDFParser", StubParser)
        monkeypatch.setattr("pdfminer.pdfdocument.PDFDocument", StubDocument)

        with pytest.raises(PDFTextExtractionNotAllowed, match="Text extraction is not allowed"):
            list(PDFPage.get_pages(BytesIO(b"%PDF-1.4\n"), check_extractable=True))

    def test_get_pages_warns_when_document_is_not_extractable(self, monkeypatch, caplog):
        from io import BytesIO
        from pdfminer.pdfpage import PDFPage

        class StubParser:
            def __init__(self, fp):
                self.fp = fp

        class StubDocument:
            def __init__(self, parser, password=b"", caching=True):
                self.parser = parser
                self.password = password
                self.caching = caching
                self.is_extractable = False

        monkeypatch.setattr("pdfminer.pdfparser.PDFParser", StubParser)
        monkeypatch.setattr("pdfminer.pdfdocument.PDFDocument", StubDocument)
        monkeypatch.setattr(PDFPage, "create_pages", classmethod(lambda cls, doc: iter(())))

        with caplog.at_level("WARNING"):
            pages = list(PDFPage.get_pages(BytesIO(b"%PDF-1.4\n"), check_extractable=False))

        assert pages == []
        assert "should not allow text extraction" in caplog.text

    def test_page_attrs_resolved(self):
        """PDFPage attrs should have resolved values for pdfplumber compatibility."""
        from pdfminer.pdfparser import PDFParser
        from pdfminer.pdfdocument import PDFDocument
        from pdfminer.pdfpage import PDFPage
        from pdfminer.pdftypes import PDFStream

        pdf_path = FIXTURES_DIR / "simple1.pdf"
        with open(pdf_path, "rb") as f:
            parser = PDFParser(f)
            doc = PDFDocument(parser)
            page = next(PDFPage.create_pages(doc))
            contents = page.attrs.get("Contents")
            # Attrs are resolved for pdfplumber compatibility (e.g., Rotate % 360)
            if isinstance(contents, list):
                assert contents, "Expected non-empty Contents list"
                assert isinstance(contents[0], PDFStream)
            else:
                assert isinstance(contents, PDFStream)

    def test_page_attrs_require_rust_source(self):
        """PDFPage should require Rust attrs as the single source of truth."""
        from pdfminer.pdfpage import PDFPage

        class DummyPage:
            pageid = 1
            mediabox = (0.0, 0.0, 1.0, 1.0)
            cropbox = None
            rotate = 0
            resources = {}
            label = None
            annots = None
            bleedbox = None
            trimbox = None
            artbox = None

        with pytest.raises(AttributeError):
            PDFPage(DummyPage(), object())

    def test_page_init_is_lazy_for_expensive_fields(self):
        """PDFPage init should not resolve attrs/resources/annots eagerly."""
        from pdfminer.pdfpage import PDFPage

        class DummyPage:
            pageid = 1
            mediabox = (0.0, 0.0, 1.0, 1.0)
            cropbox = None
            rotate = 0
            label = None
            bleedbox = None
            trimbox = None
            artbox = None

            @property
            def resources(self):
                raise AssertionError("resources resolved eagerly")

            @property
            def annots(self):
                raise AssertionError("annots resolved eagerly")

            @property
            def attrs(self):
                raise AssertionError("attrs resolved eagerly")

        class DummyDoc:
            _rust_doc = object()

        page = PDFPage(DummyPage(), DummyDoc())
        assert page.pageid == 1

    def test_page_expensive_fields_resolve_lazily_and_cache(self):
        """PDFPage should resolve attrs/resources/annots on first access only."""
        from pdfminer.pdfpage import PDFPage

        class DummyPage:
            pageid = 1
            mediabox = (0.0, 0.0, 1.0, 1.0)
            cropbox = None
            rotate = 0
            label = None
            bleedbox = None
            trimbox = None
            artbox = None

            def __init__(self):
                self.resources_calls = 0
                self.annots_calls = 0
                self.attrs_calls = 0

            @property
            def resources(self):
                self.resources_calls += 1
                return {"Font": {}}

            @property
            def annots(self):
                self.annots_calls += 1
                return [{"Rect": [0, 0, 1, 1]}]

            @property
            def attrs(self):
                self.attrs_calls += 1
                return {"MediaBox": [0, 0, 1, 1]}

        class DummyDoc:
            _rust_doc = object()

        rust_page = DummyPage()
        page = PDFPage(rust_page, DummyDoc())
        assert rust_page.resources_calls == 0
        assert rust_page.annots_calls == 0
        assert rust_page.attrs_calls == 0

        assert page.resources == {"Font": {}}
        assert page.resources == {"Font": {}}
        assert rust_page.resources_calls == 1

        assert page.annots == [{"Rect": [0, 0, 1, 1]}]
        assert page.annots == [{"Rect": [0, 0, 1, 1]}]
        assert rust_page.annots_calls == 1

        assert page.attrs == {"MediaBox": [0, 0, 1, 1]}
        assert page.attrs == {"MediaBox": [0, 0, 1, 1]}
        assert rust_page.attrs_calls == 1

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
                assert isinstance(annot, dict), (
                    f"Annotation should be dict, got {type(annot)}"
                )
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
                        if hasattr(item, "graphicstate") and item.graphicstate:
                            ncolor = item.graphicstate.ncolor
                            # Should NOT be default (0) for colored text
                            if ncolor != 0 and ncolor != (0,):
                                chars_with_color.append(item)

                # pdffill-demo.pdf should have some colored text
                assert len(chars_with_color) > 0, (
                    "Expected some chars with non-default colors"
                )
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
                        if hasattr(item, "graphicstate") and item.graphicstate:
                            nc = item.graphicstate.ncolor
                            if isinstance(nc, tuple) and len(nc) == 3:
                                r, g, b = nc
                                if r > 0.9 and g < 0.1 and b < 0.1:
                                    red_chars.append(item.get_text())

                # Should find the red "November - 2015" text
                red_text = "".join(red_chars)
                assert "November" in red_text, (
                    f"Expected 'November' in red text, got: {red_text}"
                )
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
                assert len(rects) > 0, (
                    "Should extract LTRect objects from pdffill-demo.pdf"
                )
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
                assert r > 0.9 and g < 0.1 and b < 0.1, (
                    f"Expected red color, got RGB{color}"
                )


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
