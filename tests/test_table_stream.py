from pathlib import Path

from bolivar import PDFDocument
import bolivar._bolivar as _bolivar


def test_table_stream_yields_results():
    pdf_path = Path("crates/core/tests/fixtures/simple1.pdf")
    doc = PDFDocument.from_path(str(pdf_path))
    boxes = doc.page_mediaboxes()
    geoms = []
    running = 0.0
    for box in boxes:
        geoms.append((tuple(box), tuple(box), running, False))
        running += box[3] - box[1]

    stream = _bolivar.extract_tables_stream_from_document(doc, geoms)
    page_idx, tables = next(iter(stream))
    assert page_idx == 0
    assert isinstance(tables, list)
