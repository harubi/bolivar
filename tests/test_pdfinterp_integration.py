from io import BytesIO
from pathlib import Path

import pytest
from pdfminer.high_level import extract_text
from pdfminer.pdftypes import PDFStream
from pdfminer.pdfexceptions import PDFNotImplementedError
from pdfminer.layout import LTTextBoxHorizontal
from pdfminer.pdfdevice import PDFDevice, TagExtractor
from pdfminer.pdfinterp import PDFResourceManager, PDFPageInterpreter
from pdfminer.pdfpage import PDFPage
from pdfminer.pdfparser import PDFParser
from pdfminer.pdfdocument import PDFDocument
from pdfminer.converter import PDFPageAggregator

FIXTURES = Path(__file__).resolve().parents[1] / "crates/core/tests/fixtures"


def build_pdf_with_content_filter(filter_name: str, raw_contents: bytes) -> bytes:
    out = bytearray(b"%PDF-1.4\n")
    offsets: list[int] = []

    def push_obj(data: bytes) -> None:
        offsets.append(len(out))
        out.extend(data)

    push_obj(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n")
    push_obj(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n")
    push_obj(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>\nendobj\n"
    )

    offsets.append(len(out))
    out.extend(
        f"4 0 obj\n<< /Length {len(raw_contents)} /Filter /{filter_name} >>\nstream\n".encode()
    )
    out.extend(raw_contents)
    out.extend(b"\nendstream\nendobj\n")

    xref_pos = len(out)
    out.extend(f"xref\n0 {len(offsets) + 1}\n0000000000 65535 f \n".encode())
    for offset in offsets:
        out.extend(f"{offset:010} 00000 n \n".encode())
    out.extend(
        f"trailer\n<< /Size {len(offsets) + 1} /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF".encode()
    )
    return bytes(out)


def _load_first_page():
    pdf_path = FIXTURES / "simple1.pdf"
    with open(pdf_path, "rb") as fp:
        parser = PDFParser(fp)
        doc = PDFDocument(parser)
    page = next(PDFPage.create_pages(doc))
    return doc, page


def _load_first_page_from_bytes(pdf_bytes: bytes):
    parser = PDFParser(BytesIO(pdf_bytes))
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


def test_interpreter_without_laparams_keeps_raw_layout_items():
    _, page = _load_first_page()
    rsrc = PDFResourceManager()
    device = PDFPageAggregator(rsrc)
    interp = PDFPageInterpreter(rsrc, device)

    interp.process_page(page)
    layout = device.get_result()

    assert layout is not None
    assert not any(isinstance(obj, LTTextBoxHorizontal) for obj in layout._objs)


def test_interpreter_supports_tag_extractor_devices():
    _, page = _load_first_page()
    rsrc = PDFResourceManager()
    out = BytesIO()
    device = TagExtractor(rsrc, out)
    interp = PDFPageInterpreter(rsrc, device)

    interp.process_page(page)

    assert out.getvalue() == (
        b'<page id="0" bbox="0.000,0.000,612.000,792.000" rotate="0">'
        b"Hello WorldHello WorldHello WorldHello World</page>\n"
    )


def test_interpreter_supports_plain_pdfdevice_subclasses():
    _, page = _load_first_page()
    rsrc = PDFResourceManager()

    class Recorder(PDFDevice):
        def __init__(self, rsrcmgr: PDFResourceManager) -> None:
            super().__init__(rsrcmgr)
            self.events: list[tuple[str, object]] = []

        def begin_page(self, page: object, ctm: tuple[float, ...]) -> None:
            self.events.append(("begin", ctm))

        def end_page(self, page: object) -> None:
            self.events.append(("end", getattr(page, "rotate", None)))

    device = Recorder(rsrc)
    interp = PDFPageInterpreter(rsrc, device)

    interp.process_page(page)

    assert device.ctm == (1, 0, 0, 1, -0.0, -0.0)
    assert device.events == [
        ("begin", (1, 0, 0, 1, -0.0, -0.0)),
        ("end", 0),
    ]


def test_page_contents_matches_upstream_shape():
    _, page = _load_first_page()

    assert isinstance(page.contents, list)
    assert len(page.contents) == 1
    assert isinstance(page.contents[0], PDFStream)


def test_interpreter_render_contents_initializes_state():
    _, page = _load_first_page()
    rsrc = PDFResourceManager()
    device = PDFDevice(rsrc)
    interp = PDFPageInterpreter(rsrc, device)

    interp.render_contents(page.resources, page.contents)

    assert device.ctm == (1, 0, 0, 1, 0, 0)


def test_interpreter_pop_matches_upstream_stack_behavior():
    rsrc = PDFResourceManager()
    device = PDFDevice(rsrc)
    interp = PDFPageInterpreter(rsrc, device)

    interp.push(1)
    interp.push(2)

    assert interp.pop(0) == []
    assert interp.pop(1) == [2]
    assert interp.pop(1) == [1]


def test_interpreter_dup_reuses_device_and_resources():
    rsrc = PDFResourceManager()
    device = PDFDevice(rsrc)
    interp = PDFPageInterpreter(rsrc, device)

    dup = interp.dup()

    assert isinstance(dup, PDFPageInterpreter)
    assert dup is not interp
    assert dup.rsrcmgr is interp.rsrcmgr
    assert dup.device is interp.device


def test_interpreter_does_not_cache_pages():
    doc, page = _load_first_page()
    rsrc = PDFResourceManager()
    device = PDFPageAggregator(rsrc)
    interp = PDFPageInterpreter(rsrc, device)
    assert not hasattr(doc, "_layout_cache")
    interp.process_page(page)
    assert not hasattr(doc, "_layout_cache")


def test_interpreter_unsupported_filter_raises_pdf_not_implemented():
    pdf_bytes = build_pdf_with_content_filter("BogusDecode", b"BT /F1 12 Tf (Hello) Tj ET")
    _, page = _load_first_page_from_bytes(pdf_bytes)
    rsrc = PDFResourceManager()
    device = PDFPageAggregator(rsrc)
    interp = PDFPageInterpreter(rsrc, device)

    with pytest.raises(PDFNotImplementedError):
        interp.process_page(page)


def test_extract_text_unsupported_filter_raises_pdf_not_implemented():
    pdf_bytes = build_pdf_with_content_filter("BogusDecode", b"BT /F1 12 Tf (Hello) Tj ET")

    with pytest.raises(PDFNotImplementedError):
        extract_text(BytesIO(pdf_bytes))
