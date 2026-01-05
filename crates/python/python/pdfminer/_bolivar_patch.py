import os
import sys
import importlib.abc
import importlib.machinery
import importlib.util


def _should_patch() -> bool:
    val = os.getenv("BOLIVAR_PDFPLUMBER_PATCH")
    if val is None:
        return True
    return val not in {"0", "false", "False", "no", "NO"}


def _apply_patch(module) -> bool:
    page_mod = getattr(module, "page", None)
    if page_mod is None and getattr(module, "__name__", "") == "pdfplumber.page":
        page_mod = module
        pkg = sys.modules.get("pdfplumber")
        if pkg is not None and not hasattr(pkg, "page"):
            pkg.page = module
            module = pkg

    if page_mod is None:
        return False

    if getattr(page_mod.Page.extract_tables, "_bolivar_patched", False):
        return True

    from bolivar import (
        extract_table_from_page,
        extract_table_from_page_filtered,
        extract_tables_from_page,
        extract_tables_from_page_filtered,
    )

    def _extract_tables(self, table_settings=None):
        if hasattr(self, "filter_fn") or not getattr(self, "is_original", True):
            return extract_tables_from_page_filtered(self, table_settings)
        page_index = getattr(self.page_obj, "_page_index", self.page_number - 1)
        force_crop = not getattr(self, "is_original", True)
        return extract_tables_from_page(
            self.page_obj.doc._rust_doc,
            page_index,
            self.bbox,
            self.mediabox,
            self.initial_doctop,
            table_settings,
            force_crop=force_crop,
        )

    def _extract_table(self, table_settings=None):
        if hasattr(self, "filter_fn") or not getattr(self, "is_original", True):
            return extract_table_from_page_filtered(self, table_settings)
        page_index = getattr(self.page_obj, "_page_index", self.page_number - 1)
        force_crop = not getattr(self, "is_original", True)
        return extract_table_from_page(
            self.page_obj.doc._rust_doc,
            page_index,
            self.bbox,
            self.mediabox,
            self.initial_doctop,
            table_settings,
            force_crop=force_crop,
        )

    _extract_tables._bolivar_patched = True
    page_mod.Page.extract_tables = _extract_tables
    _extract_table._bolivar_patched = True
    page_mod.Page.extract_table = _extract_table
    return True


_HOOK_INSTALLED = False


class _PdfplumberPatchLoader(importlib.abc.Loader):
    def __init__(self, loader):
        self.loader = loader

    def create_module(self, spec):
        if hasattr(self.loader, "create_module"):
            return self.loader.create_module(spec)
        return None

    def exec_module(self, module):
        self.loader.exec_module(module)
        try:
            _apply_patch(module)
        finally:
            _remove_hook()


class _PdfplumberPatchFinder(importlib.abc.MetaPathFinder):
    def __init__(self, names):
        self.names = set(names)

    def find_spec(self, fullname, path, target=None):
        if fullname not in self.names:
            return None
        spec = importlib.machinery.PathFinder.find_spec(fullname, path)
        if spec is None or spec.loader is None:
            return spec
        spec.loader = _PdfplumberPatchLoader(spec.loader)
        return spec


def _install_hook(names=None):
    global _HOOK_INSTALLED
    if _HOOK_INSTALLED:
        return
    if names is None:
        names = {"pdfplumber", "pdfplumber.page"}
    sys.meta_path.insert(0, _PdfplumberPatchFinder(names))
    _HOOK_INSTALLED = True


def _remove_hook():
    global _HOOK_INSTALLED
    if not _HOOK_INSTALLED:
        return
    sys.meta_path = [
        m for m in sys.meta_path if not isinstance(m, _PdfplumberPatchFinder)
    ]
    _HOOK_INSTALLED = False


def patch_pdfplumber() -> bool:
    if not _should_patch():
        return False
    module = sys.modules.get("pdfplumber")
    if module is not None and hasattr(module, "page"):
        return _apply_patch(module)

    if module is not None:
        _install_hook({"pdfplumber.page"})
        return False

    _install_hook()
    return False
