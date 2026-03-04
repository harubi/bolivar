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


def test_pdfplumber_pages_supports_index_objects(monkeypatch):
    class _Index:
        def __init__(self, value):
            self.value = value

        def __index__(self):
            return self.value

    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        pages = pdf.pages
        first = pages[_Index(0)]
        last = pages[_Index(-1)]
        assert first.page_number == 1
        assert last.page_number == len(pages)


def test_page_init_prefers_direct_boxes_without_attrs(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)

    class DummyPageObj:
        rotate = 0
        mediabox = (0.0, 0.0, 100.0, 200.0)
        cropbox = (0.0, 0.0, 100.0, 200.0)
        trimbox = None
        bleedbox = None
        artbox = None

        @property
        def attrs(self):
            raise AssertionError(
                "Page.__init__ should not touch attrs for direct boxes"
            )

    page = pdfplumber.page.Page(
        pdf=object(),
        page_obj=DummyPageObj(),
        page_number=1,
        initial_doctop=0,
    )
    assert page.page_number == 1
    assert page.mediabox == (0.0, 0.0, 100.0, 200.0)
    assert page.cropbox == (0.0, 0.0, 100.0, 200.0)


def test_pdfplumber_pdf_is_iterable(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        iterated = list(pdf)
        assert [p.page_number for p in iterated] == [p.page_number for p in pdf.pages]


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

    assert not hasattr(bolivar, "extract_tables_from_document")
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        page0 = pdf.pages[0]
        _ = page0.extract_tables()


def test_extract_tables_calls_indexed_single_page_backend(monkeypatch):
    import bolivar._native_api as native_api

    calls = {"stream_count": 0, "indexed": []}

    def _fake_extract_tables_stream(*args, **kwargs):
        calls["stream_count"] += 1
        return iter(())

    def _fake_extract_tables_for_page_indexed(
        doc, page_index, geometry, table_settings=None, laparams=None, caching=True
    ):
        calls["indexed"].append(
            {
                "doc": doc,
                "page_index": page_index,
                "geometry": geometry,
                "table_settings": table_settings,
                "laparams": laparams,
                "caching": caching,
            }
        )
        return [[["indexed"]]]

    monkeypatch.setattr(
        native_api, "_extract_tables_stream", _fake_extract_tables_stream
    )
    monkeypatch.setattr(
        native_api,
        "_extract_tables_for_page_indexed",
        _fake_extract_tables_for_page_indexed,
    )
    pdfplumber = _reload_pdfplumber(monkeypatch)

    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        expected_page_index = getattr(
            page.page_obj, "_page_index", page.page_number - 1
        )
        got = page.extract_tables({"vertical_strategy": "lines"})

    assert got == [[["indexed"]]]
    assert calls["stream_count"] == 0
    assert len(calls["indexed"]) == 1
    call = calls["indexed"][0]
    assert call["page_index"] == expected_page_index
    assert call["geometry"] == (
        tuple(page.bbox),
        tuple(page.mediabox),
        float(page.initial_doctop),
        False,
    )
    assert call["table_settings"] == {"vertical_strategy": "lines"}


def test_extract_tables_cropped_page_uses_page_objects_backend(monkeypatch):
    import bolivar._native_api as native_api

    calls = {"indexed_count": 0, "page_objects": []}

    def _fake_extract_tables_for_page_indexed(*args, **kwargs):
        calls["indexed_count"] += 1
        return [[["indexed"]]]

    def _fake_extract_tables_from_page_objects(
        objects,
        page_bbox,
        mediabox,
        initial_doctop=0.0,
        table_settings=None,
        force_crop=False,
    ):
        calls["page_objects"].append(
            {
                "objects": objects,
                "page_bbox": page_bbox,
                "mediabox": mediabox,
                "initial_doctop": initial_doctop,
                "table_settings": table_settings,
                "force_crop": force_crop,
            }
        )
        return [[["cropped"]]]

    monkeypatch.setattr(
        native_api,
        "_extract_tables_for_page_indexed",
        _fake_extract_tables_for_page_indexed,
    )
    monkeypatch.setattr(
        native_api,
        "_extract_tables_from_page_objects",
        _fake_extract_tables_from_page_objects,
    )
    pdfplumber = _reload_pdfplumber(monkeypatch)

    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        bbox = page.bbox
        cropped = page.crop(
            (bbox[0], bbox[1], (bbox[0] + bbox[2]) / 2, (bbox[1] + bbox[3]) / 2)
        )
        got = cropped.extract_tables({"horizontal_strategy": "text"})

    assert got == [[["cropped"]]]
    assert calls["indexed_count"] == 0
    assert len(calls["page_objects"]) == 1
    call = calls["page_objects"][0]
    assert call["table_settings"] == {"horizontal_strategy": "text"}
    assert call["force_crop"] is True


def test_extract_tables_original_page_falls_back_to_page_objects_when_indexed_symbol_missing(
    monkeypatch,
):
    import bolivar._native_api as native_api

    calls = {"stream_count": 0, "page_objects": 0}

    def _fake_extract_tables_stream(*args, **kwargs):
        calls["stream_count"] += 1
        return iter(())

    def _missing_indexed(*args, **kwargs):
        raise AttributeError("missing native symbol")

    def _fake_extract_tables_from_page_objects(
        objects,
        page_bbox,
        mediabox,
        initial_doctop=0.0,
        table_settings=None,
        force_crop=False,
    ):
        del objects, page_bbox, mediabox, initial_doctop, force_crop
        calls["page_objects"] += 1
        return [[["fallback"]]]

    monkeypatch.setattr(
        native_api, "_extract_tables_stream", _fake_extract_tables_stream
    )
    monkeypatch.setattr(
        native_api, "_extract_tables_for_page_indexed", _missing_indexed
    )
    monkeypatch.setattr(
        native_api,
        "_extract_tables_from_page_objects",
        _fake_extract_tables_from_page_objects,
    )
    pdfplumber = _reload_pdfplumber(monkeypatch)

    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        got = page.extract_tables({"vertical_strategy": "lines"})

    assert got == [[["fallback"]]]
    assert calls["page_objects"] == 1
    assert calls["stream_count"] == 0


def test_extract_text_reuses_text_stream(monkeypatch):
    import bolivar._bolivar as _bolivar

    pdfplumber = _reload_pdfplumber(monkeypatch)
    calls = {"count": 0}
    target = _bolivar._extract_text_stream

    def profiler(frame, event, arg):
        if event == "c_call" and arg is target:
            calls["count"] += 1
        return profiler

    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    prior_profiler = sys.getprofile()
    sys.setprofile(profiler)
    try:
        with pdfplumber.open(pdf_path) as pdf:
            _ = pdf.pages[0].extract_text()
            _ = pdf.pages[1].extract_text()
    finally:
        sys.setprofile(prior_profiler)

    assert calls["count"] == 2


def test_extract_words_reuses_words_stream(monkeypatch):
    import bolivar._bolivar as _bolivar

    pdfplumber = _reload_pdfplumber(monkeypatch)
    calls = {"count": 0}
    target = _bolivar._extract_words_stream

    def profiler(frame, event, arg):
        if event == "c_call" and arg is target:
            calls["count"] += 1
        return profiler

    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    prior_profiler = sys.getprofile()
    sys.setprofile(profiler)
    try:
        with pdfplumber.open(pdf_path) as pdf:
            _ = pdf.pages[0].extract_words()
            _ = pdf.pages[1].extract_words()
    finally:
        sys.setprofile(prior_profiler)

    assert calls["count"] == 2


def test_extract_tables_does_not_create_table_stream_cache(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        _ = pdf.pages[0].extract_tables()
        assert not hasattr(pdf, "_bolivar_table_streams")


def test_extract_tables_rejects_threads_kw(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        with pytest.raises(TypeError):
            page.extract_tables(threads=1)


def test_pdfplumber_repair_honors_falsey_outfile(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    with pytest.raises(OSError):
        pdfplumber.repair.repair(pdf_path, outfile="")


def test_extract_tables_uses_bolivar_single_page_indexed(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    import bolivar._native_api as native_api

    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        page_index = getattr(page.page_obj, "_page_index", page.page_number - 1)
        geometry = (
            tuple(page.bbox),
            tuple(page.mediabox),
            float(page.initial_doctop),
            False,
        )
        try:
            expected = native_api._extract_tables_for_page_indexed(
                pdf.doc._rust_doc,
                page_index,
                geometry,
                laparams=pdf.laparams,
                caching=pdf.doc.caching,
            )
        except AttributeError:
            expected = native_api._extract_tables_from_page_objects(
                page.objects,
                page.bbox,
                page.mediabox,
                page.initial_doctop,
                force_crop=False,
            )
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
