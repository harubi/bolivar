import logging
import sys
import importlib.abc
import importlib.machinery
import importlib.util
import threading


def _apply_patch(module) -> bool:
    page_mod = getattr(module, "page", None)
    if page_mod is None and getattr(module, "__name__", "") == "pdfplumber.page":
        page_mod = module
        pkg = sys.modules.get("pdfplumber")
        if pkg is not None and not hasattr(pkg, "page"):
            pkg.page = module
            module = pkg
    if page_mod is None and getattr(module, "__name__", "") == "pdfplumber.pdf":
        page_mod = sys.modules.get("pdfplumber.page")
        pkg = sys.modules.get("pdfplumber")
        if pkg is not None and page_mod is not None and not hasattr(pkg, "page"):
            pkg.page = page_mod

    if page_mod is None:
        return False

    already_patched = getattr(page_mod.Page.extract_tables, "_bolivar_patched", False)

    from bolivar import (
        extract_tables_from_document,
        extract_tables_from_page_filtered,
        extract_tables_from_page,
        extract_table_from_page_filtered,
    )
    from bolivar._bolivar import extract_pages_async_from_document
    from pdfminer.layout import LTPage
    from pdfplumber.utils.exceptions import PdfminerException

    def _page_geom(page):
        return (
            tuple(page.bbox),
            tuple(page.mediabox),
            float(page.initial_doctop),
            not getattr(page, "is_original", True),
        )

    def _safe_page_mediaboxes(doc):
        try:
            return doc.page_mediaboxes()
        except PdfminerException:
            raise
        except Exception as e:
            raise PdfminerException(str(e))

    def _build_geometries(pdf, page_index, page):
        boxes = _safe_page_mediaboxes(pdf.doc)
        doctops = []
        running = 0.0
        for box in boxes:
            doctops.append(running)
            running += box[3] - box[1]
        geoms = [
            (tuple(box), tuple(box), doctop, False)
            for box, doctop in zip(boxes, doctops)
        ]
        current = _page_geom(page)
        if 0 <= page_index < len(geoms) and geoms[page_index] != current:
            geoms[page_index] = current
        return geoms

    if not already_patched:

        def _extract_tables(self, table_settings=None):
            if getattr(self, "filter_fn", None) is not None:
                return extract_tables_from_page_filtered(self, table_settings)
            page_index = getattr(self.page_obj, "_page_index", self.page_number - 1)
            pdf = getattr(self, "pdf", None)
            if pdf is None or not hasattr(pdf, "doc"):
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

            force_crop = not getattr(self, "is_original", True)
            return extract_tables_from_page(
                pdf.doc._rust_doc,
                page_index,
                self.bbox,
                self.mediabox,
                self.initial_doctop,
                table_settings,
                force_crop=force_crop,
            )

        def _table_cell_count(table):
            return sum(len(row) for row in table)

        def _extract_table(self, table_settings=None):
            if getattr(self, "filter_fn", None) is not None:
                return extract_table_from_page_filtered(self, table_settings)
            tables = _extract_tables(self, table_settings=table_settings)
            if not tables:
                return None
            return max(tables, key=_table_cell_count)

        _extract_tables._bolivar_patched = True
        page_mod.Page.extract_tables = _extract_tables
        _extract_table._bolivar_patched = True
        page_mod.Page.extract_table = _extract_table

    class BolivarLazyPages(list):
        def __init__(self, pdf):
            self._pdf = pdf
            self._doc = getattr(pdf, "doc", None)
            if self._doc is None:
                raise RuntimeError("pdf document missing")
            page_count = self._doc.page_count()
            if page_count <= 0:
                raise PdfminerException("PDF contains no pages")
            pages_to_parse = getattr(pdf, "pages_to_parse", None)
            if pages_to_parse is None:
                self._page_numbers = list(range(page_count))
            else:
                allowed = set(pages_to_parse)
                self._page_numbers = [
                    idx for idx in range(page_count) if (idx + 1) in allowed
                ]
            self._page_number_set = set(self._page_numbers)
            self._page_cache = {}
            self._doctops = None

        def _ensure_doctops(self):
            if self._doctops is not None:
                return
            boxes = _safe_page_mediaboxes(self._doc)
            doctops = []
            running = 0.0
            try:
                for page_index in self._page_numbers:
                    box = boxes[page_index]
                    height = box[3] - box[1]
                    doctops.append(running)
                    running += height
            except IndexError as e:
                raise PdfminerException(str(e))
            self._doctops = doctops

        def __len__(self):
            return len(self._page_numbers)

        def __getitem__(self, idx):
            if isinstance(idx, slice):
                return [self[i] for i in range(*idx.indices(len(self)))]
            if not isinstance(idx, int):
                raise TypeError("page index must be int or slice")
            if idx < 0:
                idx += len(self)
            if idx < 0 or idx >= len(self):
                raise IndexError("page index out of range")
            self._ensure_doctops()
            page_index = self._page_numbers[idx]
            cached = self._page_cache.get(page_index)
            if cached is not None:
                return cached
            doctop = self._doctops[idx]
            try:
                page_obj = self._doc.get_page(page_index)
            except PdfminerException:
                raise
            except Exception as e:
                raise PdfminerException(str(e))
            page = page_mod.Page(
                self._pdf,
                page_obj,
                page_number=page_index + 1,
                initial_doctop=doctop,
            )
            self._page_cache[page_index] = page
            return page

        def __iter__(self):
            for i in range(len(self)):
                yield self[i]

        def __reversed__(self):
            for i in range(len(self) - 1, -1, -1):
                yield self[i]

        def __contains__(self, item):
            if hasattr(item, "page_number"):
                return (item.page_number - 1) in self._page_number_set
            return False

        def __aiter__(self):
            async def gen():
                rust_doc = getattr(self._pdf, "_rust_doc", None) or self._doc._rust_doc
                stream = extract_pages_async_from_document(
                    rust_doc,
                    page_numbers=list(self._page_numbers),
                    maxpages=0,
                    caching=getattr(self._doc, "caching", True),
                    laparams=getattr(self._pdf, "laparams", None),
                )
                idx = 0
                async for ltpage in stream:
                    if idx >= len(self._page_numbers):
                        break
                    page = self[idx]
                    page._layout = LTPage(ltpage)
                    yield page
                    idx += 1

            return gen()

    # Always import to ensure module is fully initialized (not just in sys.modules)
    try:
        import pdfplumber.pdf as pdf_mod
    except Exception:
        pdf_mod = None

    if pdf_mod is not None and hasattr(pdf_mod, "PDF"):
        pdf_cls = pdf_mod.PDF
        current_pages = getattr(pdf_cls, "pages", None)
        current_getter = (
            current_pages.fget if isinstance(current_pages, property) else None
        )
        if current_getter is None or not getattr(
            current_getter, "_bolivar_patched", False
        ):

            def _bolivar_pages(self):
                if hasattr(self, "_pages"):
                    return self._pages
                pages = BolivarLazyPages(self)
                self._pages = pages
                return pages

            _bolivar_pages._bolivar_patched = True
            pdf_cls.pages = property(_bolivar_pages)

        if not hasattr(pdf_cls, "__aenter__"):

            async def _aenter(self):
                return self

            async def _aexit(self, exc_type, exc, tb):
                self.close()
                return False

            pdf_cls.__aenter__ = _aenter
            pdf_cls.__aexit__ = _aexit

    # Check if PDF.pages was successfully patched
    pdf_pages_patched = False
    try:
        import pdfplumber.pdf as _check_pdf

        if hasattr(_check_pdf, "PDF"):
            _pages_prop = getattr(_check_pdf.PDF, "pages", None)
            if isinstance(_pages_prop, property) and getattr(
                _pages_prop.fget, "_bolivar_patched", False
            ):
                pdf_pages_patched = True
    except Exception:
        pass

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

    # Only consider patch complete when PDF.pages is patched
    return pdf_pages_patched


_HOOK_INSTALLED = False
_HOOK_LOCK = threading.Lock()
_PATCH_APPLIED = False
_logger = logging.getLogger("bolivar.pdfplumber_patch")


class _PdfplumberPatchLoader(importlib.abc.Loader):
    def __init__(self, loader):
        self.loader = loader

    def create_module(self, spec):
        if hasattr(self.loader, "create_module"):
            return self.loader.create_module(spec)
        return None

    def exec_module(self, module):
        global _PATCH_APPLIED
        self.loader.exec_module(module)
        with _HOOK_LOCK:
            if _PATCH_APPLIED:
                return
            try:
                if _apply_patch(module):
                    _PATCH_APPLIED = True
                    _remove_hook_unlocked()  # Only remove hook after successful patch
            except Exception as e:
                _logger.warning("Failed to apply bolivar patch to pdfplumber: %s", e)
                _remove_hook_unlocked()  # Remove on error to prevent infinite retries


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
    if names is None:
        names = {"pdfplumber", "pdfplumber.page", "pdfplumber.pdf"}
    with _HOOK_LOCK:
        if _PATCH_APPLIED:
            return
        if _HOOK_INSTALLED:
            for finder in sys.meta_path:
                if isinstance(finder, _PdfplumberPatchFinder):
                    finder.names.update(names)
            return
        sys.meta_path.insert(0, _PdfplumberPatchFinder(names))
        _HOOK_INSTALLED = True


def _remove_hook_unlocked():
    global _HOOK_INSTALLED
    if not _HOOK_INSTALLED:
        return
    sys.meta_path = [
        m for m in sys.meta_path if not isinstance(m, _PdfplumberPatchFinder)
    ]
    _HOOK_INSTALLED = False


def _remove_hook():
    with _HOOK_LOCK:
        _remove_hook_unlocked()


def patch_pdfplumber() -> bool:
    module = sys.modules.get("pdfplumber")
    if module is not None and hasattr(module, "page"):
        return _apply_patch(module)

    if module is not None:
        _install_hook({"pdfplumber.page", "pdfplumber.pdf"})
        return False

    _install_hook()
    return False
