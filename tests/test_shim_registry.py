import sys
import types

import pytest

def _clear_pdfplumber_modules() -> None:
    for name in list(sys.modules.keys()):
        if name == "pdfplumber" or name.startswith("pdfplumber."):
            sys.modules.pop(name, None)


def test_shim_registry_raises_when_patch_missing():
    _clear_pdfplumber_modules()
    fake_page = types.SimpleNamespace()
    fake_page.Page = types.SimpleNamespace(extract_tables=lambda *args, **kwargs: None)
    sys.modules["pdfplumber.page"] = fake_page
    sys.modules["pdfplumber"] = types.SimpleNamespace(page=fake_page)

    try:
        import bolivar._shim_registry as shim_registry

        with pytest.raises(RuntimeError, match="pdfplumber patch not applied"):
            shim_registry.ensure_pdfplumber_patched()
    finally:
        _clear_pdfplumber_modules()


def test_internal_bridge_exports_match_manifest():
    import bolivar._bridge_api as bridge
    import bolivar._native_api as native
    from bolivar._export_manifest import BRIDGE_EXPORTS

    assert tuple(bridge.__all__) == BRIDGE_EXPORTS
    assert not hasattr(bridge, "_extract_tables_stream")
    assert not hasattr(bridge, "_extract_tables_from_page_objects")
    assert not hasattr(bridge, "_extract_words_stream")
    assert set(BRIDGE_EXPORTS).isdisjoint(native.__all__)
    for name in BRIDGE_EXPORTS:
        assert not hasattr(native, name)
        assert hasattr(bridge, name)


def test_compat_table_helper_stays_bridge_only() -> None:
    import bolivar._bridge_api as bridge
    import bolivar._native_api as native

    name = "_extract_tables_for_compat_page"

    assert name in bridge.__all__
    assert name not in native.__all__
    assert hasattr(bridge, name)
    assert not hasattr(native, name)
