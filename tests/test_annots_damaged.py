"""A damaged /Annots entry that is not a dictionary is skipped."""

from pathlib import Path

import pdfplumber

FUZZ = (
    Path(__file__).parent.parent / "references/pdfplumber/tests/pdfs/from-oss-fuzz/load"
)


def test_non_dictionary_annotation_is_skipped() -> None:
    with pdfplumber.open(FUZZ / "4591020179783680.pdf") as pdf:
        page = pdf.pages[0]
        annots = page.annots
        assert annots
        assert all(isinstance(annot["data"], dict) for annot in annots)
        pdf.to_dict()
