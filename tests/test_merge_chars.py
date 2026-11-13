"""pdfplumber 0.11.10: a char without text adds nothing to the word."""

from pdfplumber.utils.text import WordExtractor


def char(text: str | None, x0: float) -> dict[str, object]:
    return {
        "text": text,
        "x0": x0,
        "x1": x0 + 5,
        "top": 10,
        "bottom": 20,
        "doctop": 10,
        "upright": True,
    }


def test_merge_chars_treats_missing_text_as_empty() -> None:
    word = WordExtractor().merge_chars([char("a", 0), char(None, 5), char("b", 10)])
    assert word["text"] == "ab"
