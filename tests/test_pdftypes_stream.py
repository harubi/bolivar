from pathlib import Path

from pdfminer.pdfdocument import PDFDocument
from pdfminer.pdfparser import PDFParser
from pdfminer.pdftypes import PDFStream, stream_value

ROOT = Path(__file__).resolve().parents[1]
SAMPLE = ROOT / "references/pdfminer.six/samples/contrib/issue-1062-filters.pdf"


def test_stream_decodes_via_rust():
    with SAMPLE.open("rb") as fp:
        parser = PDFParser(fp)
        doc = PDFDocument(parser)

        stream_obj = None
        for objid in range(1, 5000):
            try:
                obj = doc.getobj(objid)
            except Exception:
                continue
            if isinstance(obj, PDFStream) and obj.attrs.get("Filter") is not None:
                stream_obj = obj
                break

        assert stream_obj is not None
        data = stream_value(stream_obj)
        assert isinstance(data, (bytes, bytearray))
        assert len(data) > 0
        assert data != stream_obj.rawdata
