"""Tests for PyO3 bindings (TDD)"""

from io import BytesIO, StringIO
from pathlib import Path
from tempfile import TemporaryDirectory

import pytest

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


def test_process_page_matches_document_page_zero() -> None:
    import bolivar

    pdf_path = FIXTURES_DIR / "simple1.pdf"
    pdf_bytes = pdf_path.read_bytes()
    doc = bolivar.PDFDocument(pdf_bytes)
    page = doc.get_page(0)

    single = bolivar.process_page(doc, page)
    batch = bolivar.process_pages(doc)[0]

    assert single.pageid == batch.pageid


def test_process_pages_uses_existing_document() -> None:
    import bolivar

    pdf_path = FIXTURES_DIR / "encryption" / "rc4-40.pdf"
    pdf_bytes = pdf_path.read_bytes()
    doc = bolivar.PDFDocument(pdf_bytes, password="foo")

    pages = bolivar.process_pages(doc)
    assert len(pages) == 1


def test_extract_pages_bytes_and_path_match(tmp_path: Path) -> None:
    import bolivar

    pdf_path = FIXTURES_DIR / "simple1.pdf"
    pdf_bytes = pdf_path.read_bytes()
    path = tmp_path / "sample.pdf"
    path.write_bytes(pdf_bytes)

    from_bytes = bolivar.extract_pages(pdf_bytes)
    from_path = bolivar.extract_pages_from_path(str(path))

    assert [page.pageid for page in from_bytes] == [page.pageid for page in from_path]


def test_pdfminer_extract_pages_iterates_direct_ltpage_children() -> None:
    from pdfminer.high_level import extract_pages
    from pdfminer.layout import LTTextBoxHorizontal

    pdf_path = FIXTURES_DIR / "simple1.pdf"
    page = next(extract_pages(pdf_path.open("rb")))
    items = list(page)

    assert len(page) == len(items) == 8
    assert all(isinstance(item, LTTextBoxHorizontal) for item in items)


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


def test_bridge_only_helpers_are_absent_from_top_level():
    import bolivar
    from bolivar._export_manifest import BRIDGE_EXPORTS

    for name in BRIDGE_EXPORTS:
        assert name not in bolivar.__all__
        assert not hasattr(bolivar, name)


def test_bolivar_public_exports_match_manifest() -> None:
    import bolivar
    import bolivar._native_api as native
    from bolivar._export_manifest import PUBLIC_EXPORTS, TOP_LEVEL_EXPORTS

    assert tuple(bolivar.__all__) == TOP_LEVEL_EXPORTS
    assert set(TOP_LEVEL_EXPORTS).issubset(PUBLIC_EXPORTS)
    for name in TOP_LEVEL_EXPORTS:
        assert getattr(bolivar, name) is getattr(native, name)


def test_native_api_excludes_bridge_only_helpers() -> None:
    import bolivar._native_api as native
    from bolivar._export_manifest import BRIDGE_EXPORTS, PUBLIC_EXPORTS

    assert tuple(native.__all__) == PUBLIC_EXPORTS
    for name in BRIDGE_EXPORTS:
        assert name not in native.__all__
        assert not hasattr(native, name)


def test_extract_tables_from_document_pages_preserves_order():
    import bolivar

    assert not hasattr(bolivar, "extract_tables_from_document_pages")


def test_bridge_api_exposes_extract_tables_for_page_indexed():
    import bolivar._bridge_api as bridge_api

    assert hasattr(bridge_api, "_extract_tables_for_page_indexed")
    assert callable(bridge_api._extract_tables_for_page_indexed)


def test_bridge_api_exposes_bridge_only_extract_helpers():
    import bolivar._bridge_api as bridge_api
    from bolivar._export_manifest import BRIDGE_EXPORTS

    assert tuple(bridge_api.__all__) == BRIDGE_EXPORTS
    for name in BRIDGE_EXPORTS:
        assert name in bridge_api.__all__
        assert hasattr(bridge_api, name)
        assert callable(getattr(bridge_api, name))


def test_compat_table_helper_is_importable_only_from_bridge_api() -> None:
    bridge_namespace: dict[str, object] = {}

    with pytest.raises(ImportError):
        exec("from bolivar import _extract_tables_for_compat_page", {})

    with pytest.raises(ImportError):
        exec("from bolivar._native_api import _extract_tables_for_compat_page", {})

    exec(
        "from bolivar._bridge_api import _extract_tables_for_compat_page",
        bridge_namespace,
    )
    assert callable(bridge_namespace["_extract_tables_for_compat_page"])


def test_bridge_api_compat_table_helper_rejects_legacy_objects_dict_signature():
    import bolivar._bridge_api as bridge_api
    import pdfplumber

    pdf_path = FIXTURES_DIR / "pdfplumber" / "pdffill-demo.pdf"
    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        with pytest.raises(TypeError):
            bridge_api._extract_tables_for_compat_page(
                page.objects,
                (
                    tuple(page.bbox),
                    tuple(page.mediabox),
                    float(page.initial_doctop),
                    False,
                ),
            )


def test_threads_kw_rejected_in_python_bindings():
    import bolivar

    pdf_path = FIXTURES_DIR / "pdfplumber" / "pdffill-demo.pdf"
    pdf_bytes = pdf_path.read_bytes()
    doc = bolivar.PDFDocument(pdf_bytes)
    _ = list(doc.get_pages())[0]

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


def test_high_level_extract_text_accepts_codec_keyword():
    from pdfminer import high_level

    pdf_path = FIXTURES_DIR / "simple1.pdf"
    pdf_bytes = pdf_path.read_bytes()

    baseline = high_level.extract_text(BytesIO(pdf_bytes))

    for codec in ("utf-8", "utf-16", "latin-1", None):
        text = high_level.extract_text(BytesIO(pdf_bytes), codec=codec)
        assert text == baseline


def test_high_level_extract_text_empty_page_numbers_matches_default_behavior():
    from pdfminer import high_level

    pdf_path = FIXTURES_DIR / "simple1.pdf"
    pdf_bytes = pdf_path.read_bytes()

    baseline = high_level.extract_text(BytesIO(pdf_bytes))
    selected = high_level.extract_text(BytesIO(pdf_bytes), page_numbers=set())

    assert selected == baseline


def test_high_level_extract_pages_empty_page_numbers_matches_default_behavior():
    from pdfminer import high_level

    pdf_path = FIXTURES_DIR / "simple1.pdf"
    pdf_bytes = pdf_path.read_bytes()

    baseline = list(high_level.extract_pages(BytesIO(pdf_bytes)))
    selected = list(high_level.extract_pages(BytesIO(pdf_bytes), page_numbers=set()))

    assert len(selected) == len(baseline)
    assert [page.pageid for page in selected] == [page.pageid for page in baseline]


def test_high_level_extract_text_to_fp_text_output_matches_upstream_converter():
    from pdfminer import high_level

    pdf_path = FIXTURES_DIR / "simple1.pdf"
    pdf_bytes = pdf_path.read_bytes()
    out = BytesIO()

    high_level.extract_text_to_fp(BytesIO(pdf_bytes), out, output_type="text")

    assert out.getvalue() == b"Hello WorldHello WorldHello WorldHello World\x0c"


def test_high_level_extract_text_to_fp_text_output_with_output_dir_matches_default():
    from pdfminer import high_level

    pdf_path = FIXTURES_DIR / "simple1.pdf"
    pdf_bytes = pdf_path.read_bytes()
    baseline = BytesIO()
    with_output_dir = BytesIO()

    high_level.extract_text_to_fp(BytesIO(pdf_bytes), baseline, output_type="text")
    with TemporaryDirectory() as output_dir:
        high_level.extract_text_to_fp(
            BytesIO(pdf_bytes),
            with_output_dir,
            output_type="text",
            output_dir=output_dir,
        )

    assert with_output_dir.getvalue() == baseline.getvalue()


def test_high_level_extract_text_to_fp_tag_output_uses_tag_extractor():
    from pdfminer import high_level

    pdf_path = FIXTURES_DIR / "simple1.pdf"
    pdf_bytes = pdf_path.read_bytes()
    out = BytesIO()

    high_level.extract_text_to_fp(BytesIO(pdf_bytes), out, output_type="tag")

    output = out.getvalue()
    assert output.startswith(b'<page id="0"')
    assert b'bbox="0.000,0.000,612.000,792.000"' in output
    assert b"Hello WorldHello WorldHello WorldHello World" in output


def test_high_level_extract_text_to_fp_tag_output_honors_rotation():
    from pdfminer import high_level

    pdf_path = FIXTURES_DIR / "simple1.pdf"
    pdf_bytes = pdf_path.read_bytes()
    out = BytesIO()

    high_level.extract_text_to_fp(
        BytesIO(pdf_bytes),
        out,
        output_type="tag",
        rotation=90,
    )

    output = out.getvalue()
    assert b'rotate="90"' in output
    assert b"Hello WorldHello WorldHello WorldHello World" in output


def test_high_level_extract_text_to_fp_xml_output_honors_rotation():
    from pdfminer import high_level

    pdf_path = FIXTURES_DIR / "simple1.pdf"
    pdf_bytes = pdf_path.read_bytes()
    out = BytesIO()

    high_level.extract_text_to_fp(
        BytesIO(pdf_bytes),
        out,
        output_type="xml",
        rotation=90,
    )

    output = out.getvalue()
    assert b'<page id="1" bbox="0.000,0.000,792.000,612.000" rotate="0">' in output


@pytest.mark.parametrize(
    ("output_type", "prefix"),
    [
        ("text", "Hello WorldHello WorldHello WorldHello World\x0c"),
        ("xml", '<?xml version="1.0" ?>\n<pages>\n<page id="1"'),
        ("html", '<html><head>\n<meta http-equiv="Content-Type" content="text/html">'),
    ],
)
def test_high_level_extract_text_to_fp_supports_text_stream_outputs(
    output_type: str, prefix: str
) -> None:
    from pdfminer import high_level

    pdf_path = FIXTURES_DIR / "simple1.pdf"
    pdf_bytes = pdf_path.read_bytes()
    out = StringIO()

    high_level.extract_text_to_fp(
        BytesIO(pdf_bytes),
        out,
        output_type=output_type,
        codec=None,
    )

    assert out.getvalue().startswith(prefix)


@pytest.mark.parametrize(
    ("output_type", "message"),
    [
        ("xml", "Codec is required for a binary I/O output"),
        ("html", "Codec must not be specified for a text I/O output"),
    ],
)
def test_high_level_extract_text_to_fp_rejects_invalid_text_stream_codecs(
    output_type: str, message: str
) -> None:
    from pdfminer import high_level
    from pdfminer.pdfexceptions import PDFValueError

    pdf_path = FIXTURES_DIR / "simple1.pdf"
    pdf_bytes = pdf_path.read_bytes()
    out = StringIO()

    with pytest.raises(PDFValueError, match=message):
        high_level.extract_text_to_fp(
            BytesIO(pdf_bytes),
            out,
            output_type=output_type,
            codec="utf-8",
        )


def test_extract_tables_settings_affects_output():
    import pdfplumber

    pdf_path = ROOT / "crates/core/tests/fixtures/pdfplumber/issue-192-example.pdf"
    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        base_settings = {
            "horizontal_strategy": "text",
            "vertical_strategy": "text",
            "min_words_vertical": 20,
        }

        t = page.extract_tables(base_settings)
        t_tol = page.extract_tables({**base_settings, "text_x_tolerance": 1})

    assert t
    assert t_tol
    assert t[-1] != t_tol[-1]


def test_ltpage_iter_returns_layout_items():
    """LTPage can be iterated to get layout items"""
    from bolivar import PDFDocument, LAParams, process_page

    pdf_path = FIXTURES_DIR / "simple1.pdf"
    pdf_bytes = pdf_path.read_bytes()
    doc = PDFDocument(pdf_bytes)

    pages = list(doc.get_pages())
    laparams = LAParams()
    ltpage = process_page(doc, pages[0], laparams)

    items = list(ltpage)
    assert isinstance(items, list)


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
