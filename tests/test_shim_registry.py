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
