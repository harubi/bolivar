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
        extract_tables_from_page_filtered,
        extract_tables_from_page,
        extract_table_from_page_filtered,
        extract_tables_stream_from_document,
    )
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

    def _base_geometries(doc):
        boxes = _safe_page_mediaboxes(doc)
        doctops = []
        running = 0.0
        for box in boxes:
            doctops.append(running)
            running += box[3] - box[1]
        return [
            (tuple(box), tuple(box), doctop, False)
            for box, doctop in zip(boxes, doctops)
        ]

    def _get_base_geometries(pdf, doc):
        if pdf is not None:
            base = getattr(pdf, "_bolivar_table_geom_base", None)
            if base is not None:
                return base
        base = _base_geometries(doc)
        if pdf is not None:
            pdf._bolivar_table_geom_base = base
        return base

    def _build_geometries(doc, page_index, page, base=None):
        if base is None:
            base = _base_geometries(doc)
        geoms = list(base)
        current = _page_geom(page)
        if 0 <= page_index < len(geoms) and geoms[page_index] != current:
            geoms[page_index] = current
        return geoms

    def _normalize_key(value):
        if isinstance(value, dict):
            return tuple(
                sorted((key, _normalize_key(val)) for key, val in value.items())
            )
        if isinstance(value, (list, tuple)):
            return tuple(_normalize_key(val) for val in value)
        return value

    def _settings_key(table_settings):
        return _normalize_key(table_settings)

    def _laparams_key(pdf):
        laparams = getattr(pdf, "laparams", None) if pdf is not None else None
        if laparams is None:
            return None
        try:
            items = getattr(laparams, "__dict__", None)
            if items:
                return tuple(sorted((k, _normalize_key(v)) for k, v in items.items()))
        except Exception:
            pass
        return repr(laparams)

    _TABLE_STREAM_CACHE_LIMIT = 2

    class _BolivarTableStream:
        def __init__(self, stream_factory, cache_limit=_TABLE_STREAM_CACHE_LIMIT):
            self._stream_factory = stream_factory
            self._stream = iter(stream_factory())
            self._cache = {}
            self._done = False
            self._lock = threading.Lock()
            self._max_index_seen = -1
            self._cache_limit = max(int(cache_limit), 0)

        def _evict_cache(self, newest_index):
            if self._cache_limit <= 0:
                self._cache.clear()
                return
            keep_from = newest_index - (self._cache_limit - 1)
            for key in list(self._cache.keys()):
                if key < keep_from:
                    self._cache.pop(key, None)

        def _get_from_fresh_stream(self, page_index):
            stream = iter(self._stream_factory())
            for idx, tables in stream:
                if idx == page_index:
                    return tables
                if idx > page_index:
                    break
            return None

        def get(self, page_index):
            with self._lock:
                if page_index in self._cache:
                    return self._cache[page_index]
                if page_index < self._max_index_seen or self._done:
                    return self._get_from_fresh_stream(page_index)
                while page_index not in self._cache:
                    try:
                        idx, tables = next(self._stream)
                    except StopIteration:
                        self._done = True
                        break
                    self._cache[idx] = tables
                    if idx > self._max_index_seen:
                        self._max_index_seen = idx
                    self._evict_cache(idx)
                return self._cache.get(page_index)

    def _get_table_stream(pdf, doc, table_settings, geometries, page_numbers):
        streams = None
        if pdf is not None:
            streams = getattr(pdf, "_bolivar_table_streams", None)
            if streams is None:
                streams = {}
                pdf._bolivar_table_streams = streams
        settings_key = _settings_key(table_settings)
        geometries_key = tuple(geometries)
        laparams_key = _laparams_key(pdf)
        page_numbers_key = tuple(page_numbers) if page_numbers is not None else None
        key = (settings_key, geometries_key, laparams_key, page_numbers_key)
        if streams is not None and key in streams:
            return streams[key]
        rust_doc = getattr(doc, "_rust_doc", None) or doc

        def _stream_factory():
            return extract_tables_stream_from_document(
                rust_doc,
                geometries,
                table_settings=table_settings,
                laparams=getattr(pdf, "laparams", None) if pdf is not None else None,
                page_numbers=page_numbers,
                maxpages=0,
                caching=getattr(doc, "caching", True),
            )

        wrapped = _BolivarTableStream(_stream_factory)
        if streams is not None:
            streams[key] = wrapped
        return wrapped

    if not already_patched:

        def _extract_tables(self, table_settings=None):
            if getattr(self, "filter_fn", None) is not None:
                return extract_tables_from_page_filtered(self, table_settings)
            page_index = getattr(self.page_obj, "_page_index", self.page_number - 1)
            pdf = getattr(self, "pdf", None)
            doc = getattr(pdf, "doc", None) if pdf is not None else None
            if doc is None:
                doc = getattr(self.page_obj, "doc", None)
            if doc is None:
                raise PdfminerException("pdf document missing")
            base_geoms = _get_base_geometries(pdf, doc)
            geoms = _build_geometries(doc, page_index, self, base=base_geoms)
            pages = getattr(pdf, "pages", None) if pdf is not None else None
            page_numbers = (
                list(getattr(pages, "_page_numbers", [])) if pages is not None else None
            )
            stream = _get_table_stream(pdf, doc, table_settings, geoms, page_numbers)
            _unused = extract_tables_from_page
            tables = stream.get(page_index)
            return tables or []

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

        def close(self):
            for page in self._page_cache.values():
                if hasattr(page, "close"):
                    page.close()
            self._page_cache.clear()

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
                # Keep async iteration lightweight; avoid eager layout extraction to cap memory.
                self._ensure_doctops()
                page_numbers = list(self._page_numbers)
                doctops = list(self._doctops or [])
                for idx, page_index in enumerate(page_numbers):
                    cached = self._page_cache.get(page_index)
                    if cached is not None:
                        page = cached
                    else:
                        try:
                            page_obj = self._doc.get_page(page_index)
                        except PdfminerException:
                            raise
                        except Exception as e:
                            raise PdfminerException(str(e))
                        doctop = doctops[idx] if idx < len(doctops) else 0.0
                        page = page_mod.Page(
                            self._pdf,
                            page_obj,
                            page_number=page_index + 1,
                            initial_doctop=doctop,
                        )
                    yield page

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

        current_close = getattr(pdf_cls, "close", None)
        if current_close is None or not getattr(
            current_close, "_bolivar_patched", False
        ):

            def _bolivar_close(self):
                pages = getattr(self, "_pages", None)
                if pages is not None:
                    if hasattr(pages, "close"):
                        pages.close()
                    else:
                        for page in pages:
                            page.close()
                self.flush_cache()
                if not getattr(self, "stream_is_external", False):
                    self.stream.close()

            _bolivar_close._bolivar_patched = True
            pdf_cls.close = _bolivar_close

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
