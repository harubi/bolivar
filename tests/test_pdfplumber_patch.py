import importlib
import os
import sys

import pytest

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
PYTHON_SHIM = os.path.join(ROOT, "crates", "python", "python")


def _reload_pdfplumber(monkeypatch):
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

    import pdfplumber

    importlib.reload(pdfplumber)
    return pdfplumber


def test_pdfplumber_patch_default_on(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
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


def test_pdfplumber_patch_ignores_env_opt_out(monkeypatch):
    monkeypatch.setenv("BOLIVAR_PDFPLUMBER_PATCH", "0")
    pdfplumber = _reload_pdfplumber(monkeypatch)
    assert (
        getattr(pdfplumber.page.Page.extract_tables, "_bolivar_patched", False) is True
    )


def test_pdfplumber_pages_is_lazy_and_supports_slices(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        pages = pdf.pages
        # BolivarLazyPages is a list subclass for isinstance compatibility
        assert isinstance(pages, list)
        assert hasattr(pages, "_page_cache")  # But it's still lazy
        assert len(pages) >= 2
        assert pages[-1].page_number == len(pages)
        assert len(pages[1:3]) == 2


def test_pdfplumber_close_does_not_iterate_lazy_pages(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    pdf = pdfplumber.open(pdf_path)
    pages = pdf.pages

    def _boom(self):
        raise AssertionError("lazy pages iterated on close")

    monkeypatch.setattr(type(pages), "__iter__", _boom, raising=True)
    pdf.close()


def test_extract_tables_does_not_cache(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        page0 = pdf.pages[0]
        _ = page0.extract_tables()
        assert not hasattr(pdf, "_bolivar_tables_cache")
        assert not hasattr(pdf, "_bolivar_page_geometries")


def test_extract_tables_does_not_instantiate_all_pages(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    import pdfplumber.page as page_mod

    calls = {"count": 0}
    original_init = page_mod.Page.__init__

    def _counting_init(self, *args, **kwargs):
        calls["count"] += 1
        return original_init(self, *args, **kwargs)

    monkeypatch.setattr(page_mod.Page, "__init__", _counting_init)

    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        page0 = pdf.pages[0]
        calls["count"] = 0
        _ = page0.extract_tables()
        assert calls["count"] == 0


def test_extract_tables_avoids_document_wide_extraction(monkeypatch):
    import bolivar

    def _boom(*args, **kwargs):
        raise RuntimeError("doc_extraction_called")

    monkeypatch.setattr(bolivar, "extract_tables_from_document", _boom)
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        page0 = pdf.pages[0]
        _ = page0.extract_tables()


def test_extract_tables_uses_stream_not_per_page(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    fn = pdfplumber.page.Page.extract_tables
    freevars = dict(zip(fn.__code__.co_freevars, fn.__closure__))
    target = freevars["extract_tables_from_page"].cell_contents
    calls = {"count": 0}

    def profiler(frame, event, arg):
        if event == "c_call" and arg is target:
            calls["count"] += 1
        return profiler

    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    sys.setprofile(profiler)
    try:
        with pdfplumber.open(pdf_path) as pdf:
            page = pdf.pages[0]
            page.extract_tables()
    finally:
        sys.setprofile(None)

    assert calls["count"] == 0


def test_extract_tables_reuses_table_stream(monkeypatch):
    import bolivar._bolivar as _bolivar

    pdfplumber = _reload_pdfplumber(monkeypatch)
    calls = {"count": 0}
    target = _bolivar.extract_tables_stream_from_document

    def profiler(frame, event, arg):
        if event == "c_call" and arg is target:
            calls["count"] += 1
        return profiler

    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    sys.setprofile(profiler)
    try:
        with pdfplumber.open(pdf_path) as pdf:
            _ = pdf.pages[0].extract_tables()
            _ = pdf.pages[1].extract_tables()
    finally:
        sys.setprofile(None)

    assert calls["count"] == 1


def test_extract_tables_stream_cache_is_bounded(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        _ = pdf.pages[0].extract_tables()
        _ = pdf.pages[1].extract_tables()
        _ = pdf.pages[2].extract_tables()
        streams = getattr(pdf, "_bolivar_table_streams", {})
        assert streams, "expected table stream cache to be present"
        stream = next(iter(streams.values()))
        cache = getattr(stream, "_cache", {})
        assert len(cache) <= 2


def test_extract_tables_rejects_threads_kw(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "references/pdfplumber/tests/pdfs/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        with pytest.raises(TypeError):
            page.extract_tables(threads=1)


def test_extract_tables_uses_bolivar(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    import bolivar._bolivar as _bolivar

    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "references/pdfplumber/tests/pdfs/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        page_index = getattr(page.page_obj, "_page_index", page.page_number - 1)
        boxes = pdf.doc._rust_doc.page_mediaboxes()
        geometries = []
        running = 0.0
        for box in boxes:
            box = tuple(box)
            geometries.append((box, box, running, False))
            running += box[3] - box[1]
        stream = _bolivar.extract_tables_stream_from_document(
            pdf.doc._rust_doc,
            geometries,
            laparams=pdf.laparams,
            caching=pdf.doc.caching,
        )
        expected = None
        for idx, tables in stream:
            if idx == page_index:
                expected = tables
                break
        got = page.extract_tables()

    assert got == expected


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
