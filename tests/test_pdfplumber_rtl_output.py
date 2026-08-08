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
    chars = _chars_from_visual_line("ﺏﺎﺴﺤﻟﺍ ﻒﺸﻛ")
    assert pdfplumber.utils.extract_text(chars) == "كشف الحساب"


def test_page_extract_text_keeps_legacy_default_and_opts_into_bidi():
    visual_text = "ﺏﺎﺴﺤﻟﺍ ﻒﺸﻛ"

    class FakePage:
        def get_textmap(self, **kwargs):
            return type("FakeTextMap", (), {"as_string": visual_text})()

    fake_page = FakePage()
    assert Page.extract_text(fake_page) == visual_text
    assert Page.extract_text(fake_page, bidi=True) == "كشف الحساب"


def test_extract_text_keeps_ltr_segments_in_mixed_rtl_line():
    chars = _chars_from_visual_line("Account Statement ﺏﺎﺴﺤﻟﺍ ﻒﺸﻛ")
    text = pdfplumber.utils.extract_text(chars)
    assert "Account Statement" in text
    assert "tnemetatS tnuoccA" not in text
    assert "كشف الحساب" in text
