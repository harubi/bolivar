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
        extract_tables_from_document,
        extract_table_from_page,
        extract_table_from_page_filtered,
        extract_tables_from_page,
        extract_tables_from_page_filtered,
    )

    def _freeze_settings(obj):
        if obj is None:
            return None
        if isinstance(obj, dict):
            return tuple(sorted((k, _freeze_settings(v)) for k, v in obj.items()))
        if isinstance(obj, (list, tuple)):
            return tuple(_freeze_settings(v) for v in obj)
        return obj

    def _page_geom(page):
        return (
            tuple(page.bbox),
            tuple(page.mediabox),
            float(page.initial_doctop),
            not getattr(page, "is_original", True),
        )

    def _build_geometries(pdf, page_index, page):
        geoms = [_page_geom(p) for p in pdf.pages]
        current = _page_geom(page)
        if 0 <= page_index < len(geoms) and geoms[page_index] != current:
            geoms[page_index] = current
        return geoms

    def _extract_tables(self, table_settings=None, threads=None):
        if getattr(self, "filter_fn", None) is not None:
            return extract_tables_from_page_filtered(self, table_settings)
        page_index = getattr(self.page_obj, "_page_index", self.page_number - 1)
        force_crop = not getattr(self, "is_original", True)
        pdf = getattr(self, "pdf", None)
        if pdf is None or not hasattr(pdf, "pages"):
            return extract_tables_from_page(
                self.page_obj.doc._rust_doc,
                page_index,
                self.bbox,
                self.mediabox,
                self.initial_doctop,
                table_settings,
                threads=threads,
                force_crop=force_crop,
            )
        if len(pdf.pages) <= 1:
            rust_doc = getattr(pdf, "_rust_doc", None) or self.page_obj.doc._rust_doc
            return extract_tables_from_page(
                rust_doc,
                page_index,
                self.bbox,
                self.mediabox,
                self.initial_doctop,
                table_settings,
                threads=threads,
                force_crop=force_crop,
            )
        cache = getattr(pdf, "_bolivar_tables_cache", None)
        if cache is None:
            cache = {}
            pdf._bolivar_tables_cache = cache
        geoms = _build_geometries(pdf, page_index, self)
        cache_key = (_freeze_settings(table_settings), tuple(geoms))
        tables_by_page = cache.get(cache_key)
        if tables_by_page is None:
            rust_doc = getattr(pdf, "_rust_doc", None) or self.page_obj.doc._rust_doc
            tables_by_page = extract_tables_from_document(
                rust_doc,
                geoms,
                table_settings,
                threads=threads,
            )
            cache[cache_key] = tables_by_page
        return tables_by_page[page_index]

    def _table_cell_count(table):
        return sum(len(row) for row in table)

    def _extract_table(self, table_settings=None, threads=None):
        if getattr(self, "filter_fn", None) is not None:
            return extract_table_from_page_filtered(self, table_settings)
        tables = _extract_tables(self, table_settings=table_settings, threads=threads)
        if not tables:
            return None
        return max(tables, key=_table_cell_count)

    _extract_tables._bolivar_patched = True
    page_mod.Page.extract_tables = _extract_tables
    _extract_table._bolivar_patched = True
    page_mod.Page.extract_table = _extract_table

    # Patch pdfplumber.repair to use Rust repair
    try:
        import pdfplumber.repair as repair_mod
    except Exception:
        repair_mod = None

    if repair_mod is not None:
        from io import BytesIO
        from bolivar import repair_pdf

        def _rust_repair(path_or_fp, password=None, gs_path=None, setting="default"):
            # Ignore gs_path/setting; Rust repair is internal.
            return BytesIO(repair_pdf(path_or_fp))

        def _rust_repair_public(
            path_or_fp,
            outfile=None,
            password=None,
            gs_path=None,
            setting="default",
        ):
            repaired = _rust_repair(
                path_or_fp, password=password, gs_path=gs_path, setting=setting
            )
            if outfile:
                with open(outfile, "wb") as f:
                    f.write(repaired.read())
                return None
            return repaired

        repair_mod._repair = _rust_repair
        repair_mod.repair = _rust_repair_public
        if hasattr(module, "repair"):
            module.repair = repair_mod.repair
        pdf_mod = sys.modules.get("pdfplumber.pdf")
        if pdf_mod is not None and hasattr(pdf_mod, "_repair"):
            pdf_mod._repair = _rust_repair
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
