from pathlib import Path


ROOT = Path(__file__).parent.parent
FIXTURES_DIR = ROOT / "crates/core/tests/fixtures"


def test_pdfminer_ltpage_wrap_is_lazy(monkeypatch):
    from bolivar import extract_pages
    from pdfminer import layout as layout_mod

    pdf_path = FIXTURES_DIR / "simple1.pdf"
    rust_page = extract_pages(pdf_path.read_bytes())[0]

    calls = {"n": 0}
    original = layout_mod._wrap_rust_item

    def wrapped(item):
        calls["n"] += 1
        return original(item)

    monkeypatch.setattr(layout_mod, "_wrap_rust_item", wrapped)

    page = layout_mod.LTPage(rust_page)
    assert calls["n"] == 0

    _ = list(page)
    assert calls["n"] > 0
