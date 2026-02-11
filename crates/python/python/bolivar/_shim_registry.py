import importlib.util
import os
import sys
from types import ModuleType

_SHIM_PACKAGES = ("pdfminer", "pdfplumber")


def _resolve_base(base: str | None) -> str:
    if base is None:
        base = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
    return os.path.abspath(base)


def _purge_modules(prefixes: tuple[str, ...]) -> None:
    for name in list(sys.modules.keys()):
        for prefix in prefixes:
            if name == prefix or name.startswith(prefix + "."):
                sys.modules.pop(name, None)
                break


def _load_package(name: str, base: str) -> ModuleType:
    pkg_dir = os.path.join(base, name)
    init_py = os.path.join(pkg_dir, "__init__.py")
    spec = importlib.util.spec_from_file_location(
        name, init_py, submodule_search_locations=[pkg_dir]
    )
    if spec is None or spec.loader is None:
        raise ImportError(f"{name} shim not found at {init_py}")
    module = importlib.util.module_from_spec(spec)
    module.__path__ = [pkg_dir]
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def ensure_pdfplumber_patched() -> None:
    module = sys.modules.get("pdfplumber")
    if module is None:
        raise RuntimeError("pdfplumber not loaded for patch validation")
    page_mod = getattr(module, "page", None) or sys.modules.get("pdfplumber.page")
    page_cls = getattr(page_mod, "Page", None) if page_mod is not None else None
    extract_tables = getattr(page_cls, "extract_tables", None)
    if extract_tables is None or not getattr(extract_tables, "_bolivar_patched", False):
        raise RuntimeError("pdfplumber patch not applied")


def apply_pdfplumber_patch() -> None:
    import pdfminer._bolivar_patch as bolivar_patch

    bolivar_patch.patch_pdfplumber()


def install(base: str | None = None) -> dict[str, ModuleType]:
    base = _resolve_base(base)
    _purge_modules(_SHIM_PACKAGES)
    loaded = {}
    for name in _SHIM_PACKAGES:
        loaded[name] = _load_package(name, base)
    ensure_pdfplumber_patched()
    return loaded
