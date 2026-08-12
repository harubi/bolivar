import pdfplumber
from pdfplumber.page import Page


def _chars_from_visual_line(text: str) -> list[dict]:
    chars = []
    for idx, ch in enumerate(text):
        x0 = float(idx)
        chars.append(
            {
                "text": ch,
                "x0": x0,
                "x1": x0 + 1.0,
                "top": 0.0,
                "bottom": 1.0,
                "doctop": 0.0,
                "upright": True,
                "height": 1.0,
                "width": 1.0,
                "size": 10.0,
            }
        )
    return chars


def test_extract_text_normalizes_presentation_forms_and_rtl_order():
    chars = _chars_from_visual_line("ﺔﻴﺑﺮﻌﻟﺍ ﺔﻠﻤﺠﻟﺍ")
    assert pdfplumber.utils.extract_text(chars) == "الجملة العربية"


def test_page_extract_text_keeps_legacy_default_and_opts_into_bidi():
    visual_text = "ﺔﻴﺑﺮﻌﻟﺍ ﺔﻠﻤﺠﻟﺍ"

    class FakePage:
        def get_textmap(self, **kwargs):
            return type("FakeTextMap", (), {"as_string": visual_text})()

    fake_page = FakePage()
    assert Page.extract_text(fake_page) == visual_text
    assert Page.extract_text(fake_page, bidi=True) == "الجملة العربية"


def test_extract_text_keeps_ltr_segments_in_mixed_rtl_line():
    chars = _chars_from_visual_line("English text ﺔﻴﺑﺮﻌﻟﺍ")
    text = pdfplumber.utils.extract_text(chars)
    assert "English text" in text
    assert "txet hsilgnE" not in text
    assert "العربية" in text
