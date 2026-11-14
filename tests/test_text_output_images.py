"""pdf2txt's text output exports images to --output-dir, as pdfminer's does."""

import io
from pathlib import Path

from pdfminer.high_level import extract_text_to_fp

DMCA = Path(__file__).parent.parent / "crates/core/tests/fixtures/nonfree/dmca.pdf"


def text_to_fp(output_dir: str | None) -> str:
    out = io.StringIO()
    with DMCA.open("rb") as pdf:
        extract_text_to_fp(pdf, out, output_dir=output_dir, page_numbers=[0])
    return out.getvalue()


def test_text_output_exports_images_and_keeps_its_text(tmp_path: Path) -> None:
    text = text_to_fp(str(tmp_path))

    assert [p.suffix for p in tmp_path.iterdir()] == [".bmp"]
    assert text == text_to_fp(None)
