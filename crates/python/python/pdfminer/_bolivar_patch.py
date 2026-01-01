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
    if getattr(module.page.Page.extract_tables, "_bolivar_patched", False):
        return True

    from bolivar import extract_tables_from_page

    def _extract_tables(self, table_settings=None):
        page_index = getattr(self.page_obj, "_page_index", self.page_number - 1)
        return extract_tables_from_page(
            self.page_obj.doc._rust_doc,
            page_index,
            self.bbox,
            self.mediabox,
            self.initial_doctop,
            table_settings,
        )

    _extract_tables._bolivar_patched = True
    module.page.Page.extract_tables = _extract_tables
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
    def find_spec(self, fullname, path, target=None):
        if fullname != "pdfplumber":
            return None
        spec = importlib.machinery.PathFinder.find_spec(fullname, path)
        if spec is None or spec.loader is None:
            return spec
        spec.loader = _PdfplumberPatchLoader(spec.loader)
        return spec


def _install_hook():
    global _HOOK_INSTALLED
    if _HOOK_INSTALLED:
        return
    sys.meta_path.insert(0, _PdfplumberPatchFinder())
    _HOOK_INSTALLED = True


def _remove_hook():
    global _HOOK_INSTALLED
    if not _HOOK_INSTALLED:
        return
    sys.meta_path = [m for m in sys.meta_path if not isinstance(m, _PdfplumberPatchFinder)]
    _HOOK_INSTALLED = False


def patch_pdfplumber() -> bool:
    if not _should_patch():
        return False
    module = sys.modules.get("pdfplumber")
    if module is not None and hasattr(module, "page"):
        return _apply_patch(module)

    _install_hook()
    return False
