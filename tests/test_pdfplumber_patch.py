import importlib
import os
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
PYTHON_SHIM = os.path.join(ROOT, "crates", "python", "python")


def _reload_pdfplumber(monkeypatch, env_value):
    # Ensure clean import state so pdfminer/__init__.py runs
    for name in list(sys.modules.keys()):
        if name.startswith("pdfplumber") or name.startswith("pdfminer"):
            sys.modules.pop(name, None)

    if PYTHON_SHIM not in sys.path:
        sys.path.insert(0, PYTHON_SHIM)

    try:
        from bolivar import _autoload

        _autoload.install()
    except Exception:
        pass

    if env_value is None:
        monkeypatch.delenv("BOLIVAR_PDFPLUMBER_PATCH", raising=False)
    else:
        monkeypatch.setenv("BOLIVAR_PDFPLUMBER_PATCH", env_value)

    import pdfplumber

    importlib.reload(pdfplumber)
    return pdfplumber


def test_pdfplumber_patch_default_on(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch, None)
    assert (
        getattr(pdfplumber.page.Page.extract_tables, "_bolivar_patched", False) is True
    )


def test_pdfplumber_patch_default_on_without_reload(monkeypatch):
    # Clean import state so pdfminer/__init__.py runs
    for name in list(sys.modules.keys()):
        if name.startswith("pdfplumber") or name.startswith("pdfminer"):
            sys.modules.pop(name, None)

    monkeypatch.delenv("BOLIVAR_PDFPLUMBER_PATCH", raising=False)

    import pdfplumber

    assert (
        getattr(pdfplumber.page.Page.extract_tables, "_bolivar_patched", False) is True
    )


def test_pdfplumber_patch_env_opt_out(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch, "0")
    assert (
        getattr(pdfplumber.page.Page.extract_tables, "_bolivar_patched", False) is False
    )


def test_extract_tables_uses_bolivar(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch, None)
    from bolivar import extract_tables_from_page

    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "references/pdfplumber/tests/pdfs/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        page_index = getattr(page.page_obj, "_page_index", page.page_number - 1)
        expected = extract_tables_from_page(
            page.page_obj.doc._rust_doc,
            page_index,
            page.bbox,
            page.mediabox,
            page.initial_doctop,
        )
        got = page.extract_tables()

    assert got == expected


def test_extract_tables_accepts_threads(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch, None)
    from bolivar import extract_tables_from_page

    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "references/pdfplumber/tests/pdfs/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        page_index = getattr(page.page_obj, "_page_index", page.page_number - 1)
        expected = extract_tables_from_page(
            page.page_obj.doc._rust_doc,
            page_index,
            page.bbox,
            page.mediabox,
            page.initial_doctop,
            threads=1,
        )
        got = page.extract_tables(threads=1)

    assert got == expected


def test_extract_tables_sets_doc_cache(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch, None)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "references/pdfplumber/tests/pdfs/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        assert not hasattr(pdf, "_bolivar_tables_cache")
        _ = page.extract_tables()
        assert hasattr(pdf, "_bolivar_tables_cache")
        assert pdf._bolivar_tables_cache


def test_autoload_forces_shim(monkeypatch):
    for name in list(sys.modules.keys()):
        if name.startswith("pdfminer") or name.startswith("pdfplumber"):
            sys.modules.pop(name, None)

    from bolivar import _autoload

    _autoload.install()

    import pdfminer

    assert hasattr(pdfminer, "patch_pdfplumber")

    import pdfplumber

    assert (
        getattr(pdfplumber.page.Page.extract_tables, "_bolivar_patched", False) is True
    )
