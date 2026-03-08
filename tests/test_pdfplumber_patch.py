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


def test_pdfplumber_base14_font_geometry_matches_upstream(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/simple1.pdf",
    )

    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        first_char = page.chars[0]
        first_word = page.extract_words()[0]

    assert first_char["fontname"] == "Helvetica"
    assert first_char["top"] == pytest.approx(72.968, abs=1e-3)
    assert first_char["bottom"] == pytest.approx(96.968, abs=1e-3)
    assert first_char["doctop"] == pytest.approx(72.968, abs=1e-3)
    assert first_word["top"] == pytest.approx(72.968, abs=1e-3)
    assert first_word["bottom"] == pytest.approx(96.968, abs=1e-3)
    assert first_word["doctop"] == pytest.approx(72.968, abs=1e-3)


def test_pdfplumber_embedded_font_geometry_matches_upstream(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/annotations.pdf",
    )

    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        first_char = page.chars[0]
        first_word = page.extract_words()[0]
        first_match = page.search("Dummy")[0]

    assert first_char["fontname"] == "BAAAAA+Arial-BoldMT"
    assert first_char["top"] == pytest.approx(71.1971, abs=1e-3)
    assert first_char["bottom"] == pytest.approx(87.2971, abs=1e-3)
    assert first_char["doctop"] == pytest.approx(71.1971, abs=1e-3)
    assert first_word["top"] == pytest.approx(71.1971, abs=1e-3)
    assert first_word["bottom"] == pytest.approx(87.2971, abs=1e-3)
    assert first_word["doctop"] == pytest.approx(71.1971, abs=1e-3)
    assert first_match["top"] == pytest.approx(71.1971, abs=1e-3)
    assert first_match["bottom"] == pytest.approx(87.2971, abs=1e-3)


def test_pdfplumber_grayscale_non_stroking_color_matches_upstream(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/nics-background-checks-2015-11.pdf",
    )

    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        char_color = page.chars[0]["non_stroking_color"]
        word_color = page.extract_words(return_chars=True)[0]["chars"][0][
            "non_stroking_color"
        ]
        line_color = page.extract_text_lines()[0]["chars"][0]["non_stroking_color"]

    assert char_color == (0,)
    assert word_color == (0,)
    assert line_color == (0,)
    assert type(char_color[0]) is int
    assert type(word_color[0]) is int
    assert type(line_color[0]) is int


def test_pdfplumber_extract_tables_uses_single_page_native_path(monkeypatch):
    import bolivar._bridge_api as bridge_api

    calls = {"indexed": 0}

    def _fake_extract_tables_for_page_indexed(*args, **kwargs):
        del args, kwargs
        calls["indexed"] += 1
        return [[["indexed"]]]

    monkeypatch.setattr(
        bridge_api,
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
        tables = page.extract_tables()

    assert tables == [[["indexed"]]]
    assert calls["indexed"] == 1


def test_pdfplumber_extract_tables_rejects_unknown_setting(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )

    with pdfplumber.open(pdf_path) as pdf:
        with pytest.raises(TypeError):
            pdf.pages[0].extract_tables({"strategy": "x"})


@pytest.mark.parametrize(
    ("settings", "message"),
    [
        (
            {"vertical_strategy": "x"},
            "vertical_strategy must be one of{lines,lines_strict,text,explicit}",
        ),
        (
            {"vertical_strategy": "explicit", "explicit_vertical_lines": []},
            "If vertical_strategy == 'explicit'",
        ),
        (
            {"join_tolerance": -1},
            "Table setting 'join_tolerance' cannot be negative",
        ),
    ],
)
def test_pdfplumber_extract_tables_validates_settings(monkeypatch, settings, message):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )

    with pdfplumber.open(pdf_path) as pdf:
        with pytest.raises(ValueError, match=message):
            pdf.pages[0].extract_tables(settings)


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


def test_pdfplumber_pages_supports_list_operations(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/simple1.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        pages = pdf.pages
        first = pages[0]

        assert repr(pages) == repr([first])
        assert pages.copy() == [first]
        assert pages.index(first) == 0
        assert pages.count(first) == 1


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


def test_random_page_extract_tables_does_not_replay_document_stream(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        second = pdf.pages[1].extract_tables()
        first = pdf.pages[0].extract_tables()
        assert not hasattr(pdf, "_bolivar_table_streams")

    assert isinstance(second, list)
    assert isinstance(first, list)


def test_extract_tables_does_not_cache_legacy_table_geometries(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        pdf.pages[0].extract_tables()
        assert not hasattr(pdf, "_bolivar_table_geom_base")
        assert not hasattr(pdf, "_bolivar_table_geom_base_key")


def test_extract_tables_calls_indexed_backend_for_original_page(monkeypatch):
    import bolivar._bridge_api as bridge_api

    calls = {"indexed_count": 0}

    def _fake_extract_tables_for_page_indexed(*args, **kwargs):
        del args, kwargs
        calls["indexed_count"] += 1
        return [[["indexed"]]]

    monkeypatch.setattr(
        bridge_api,
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
        got = page.extract_tables({"vertical_strategy": "lines"})

    assert got == [[["indexed"]]]
    assert calls["indexed_count"] == 1


def test_extract_tables_calls_indexed_backend_for_each_original_page(monkeypatch):
    import bolivar._bridge_api as bridge_api

    calls = {"indexed_count": 0}

    def _fake_extract_tables_for_page_indexed(*args, **kwargs):
        del args, kwargs
        calls["indexed_count"] += 1
        return [[["indexed"]]]

    monkeypatch.setattr(
        bridge_api,
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
        got0 = pdf.pages[0].extract_tables({"vertical_strategy": "lines"})
        got1 = pdf.pages[1].extract_tables({"vertical_strategy": "lines"})

    assert got0 == [[["indexed"]]]
    assert got1 == [[["indexed"]]]
    assert calls["indexed_count"] == 2


def test_extract_tables_cropped_page_uses_page_objects_backend(monkeypatch):
    import bolivar._bridge_api as bridge_api

    calls = {"indexed_count": 0, "page_objects": []}

    def _fake_extract_tables_for_page_indexed(*args, **kwargs):
        calls["indexed_count"] += 1
        return [[["indexed"]]]

    def _fake_extract_tables_for_compat_page(
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
        bridge_api,
        "_extract_tables_for_page_indexed",
        _fake_extract_tables_for_page_indexed,
    )
    monkeypatch.setattr(
        bridge_api,
        "_extract_tables_for_compat_page",
        _fake_extract_tables_for_compat_page,
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


def test_extract_tables_original_page_requires_indexed_backend(
    monkeypatch,
):
    import bolivar._bridge_api as bridge_api

    calls = {"indexed_count": 0, "page_objects": 0}

    def _missing_indexed(*args, **kwargs):
        del args, kwargs
        calls["indexed_count"] += 1
        raise AttributeError("missing native symbol")

    def _fake_extract_tables_for_compat_page(
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
        bridge_api, "_extract_tables_for_page_indexed", _missing_indexed
    )
    monkeypatch.setattr(
        bridge_api,
        "_extract_tables_for_compat_page",
        _fake_extract_tables_for_compat_page,
    )
    pdfplumber = _reload_pdfplumber(monkeypatch)

    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        with pytest.raises(AttributeError, match="missing native symbol"):
            page.extract_tables({"vertical_strategy": "lines"})

    assert calls["page_objects"] == 0
    assert calls["indexed_count"] == 1


def test_repeated_page_extract_text_is_stable(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        assert page.extract_text() == page.extract_text()


def test_extract_text_matches_issue_192_upstream_lines(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/issue-192-example.pdf",
    )
    expected_lines = [
        "Agaaaaa: AAAA AAA/Aaabaaab 77-2A8A-2076",
        "AabaaAA aambaa6 618-647173-54",
        "-AOAAAAAAAAA-A",
        "Aabba7 Aabababa ab Aaaaaamaaba",
        "AAAAA",
        "20Aab 3123 Aababbaa Aaga 09ab 101",
        "Aaaabaa 8.8 Aaaaagaaabab Aababaabbab Amaabmaab 5",
        "8gabaaaaaA",
        "Ababg Aaaambaab Aaabab (Aaab Agaba = 56 Aaga) Aabbaw-ag Aaabab",
        "Agabaa 9 ba 7 Agaba 3+",
    ]

    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        lines = page.extract_text().splitlines()

    assert len(lines) == 66
    assert lines[:10] == expected_lines


def test_repeated_page_extract_words_is_stable(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        assert page.extract_words() == page.extract_words()


def test_extract_words_raises_when_native_page_output_is_missing(monkeypatch):
    import bolivar._bridge_api as bridge_api

    monkeypatch.setattr(bridge_api, "_extract_words_stream", lambda *args, **kwargs: [])
    pdfplumber = _reload_pdfplumber(monkeypatch)
    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        with pytest.raises(RuntimeError, match="missing words for page"):
            page.extract_words()


def test_extract_tables_does_not_create_table_stream_cache_for_original_pages(monkeypatch):
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


def test_extract_tables_matches_bolivar_indexed_default(monkeypatch):
    pdfplumber = _reload_pdfplumber(monkeypatch)
    import bolivar._bridge_api as bridge_api

    pdf_path = os.path.join(
        os.path.dirname(__file__),
        "..",
        "crates/core/tests/fixtures/pdfplumber/pdffill-demo.pdf",
    )
    with pdfplumber.open(pdf_path) as pdf:
        page = pdf.pages[0]
        page_index = getattr(page.page_obj, "_page_index", page.page_number - 1)
        expected = bridge_api._extract_tables_for_page_indexed(
            pdf.doc._rust_doc,
            page_index,
            (
                tuple(page.bbox),
                tuple(page.mediabox),
                float(page.initial_doctop),
                False,
            ),
            laparams=pdf.laparams,
            caching=pdf.doc.caching,
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
