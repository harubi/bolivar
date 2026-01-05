import io

from pdfminer.converter import HTMLConverter, TextConverter, XMLConverter
from pdfminer.pdfinterp import PDFPageInterpreter, PDFResourceManager
from pdfminer.pdfpage import PDFPage

SAMPLE = "references/pdfminer.six/samples/simple1.pdf"


def run_converter(conv_cls):
    rsrc = PDFResourceManager()
    out = io.BytesIO()
    device = conv_cls(rsrc, out)
    interpreter = PDFPageInterpreter(rsrc, device)
    with open(SAMPLE, "rb") as fp:
        for page in PDFPage.get_pages(fp):
            interpreter.process_page(page)
    return out.getvalue()


def test_text_converter_runs():
    assert b"Hello" in run_converter(TextConverter)


def test_html_converter_runs():
    assert b"<html" in run_converter(HTMLConverter)


def test_xml_converter_runs():
    assert b"<?xml" in run_converter(XMLConverter)
