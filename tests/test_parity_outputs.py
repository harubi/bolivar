from pathlib import Path

import pytest

from bolivar import extract_text

FIXTURES_DIR = Path(__file__).parent.parent / "crates/core/tests/fixtures"
FIXTURES = [
    "simple1.pdf",
    "simple2.pdf",
    "simple3.pdf",
    "simple4.pdf",
    "simple5.pdf",
    "jo.pdf",
]


@pytest.mark.parametrize("name", FIXTURES)
def test_parity_outputs_match_baseline(name):
    pdf_path = FIXTURES_DIR / name
    pdf_bytes = pdf_path.read_bytes()

    baseline = extract_text(pdf_bytes, caching=True)
    streamed = extract_text(pdf_bytes, caching=True)

    assert baseline == streamed
