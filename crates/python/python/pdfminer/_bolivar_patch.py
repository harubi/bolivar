import importlib.abc
import importlib.machinery
import logging
import sys
import threading
from collections.abc import AsyncIterator, Callable, Iterable, Iterator, Sequence
from io import BytesIO
from operator import index as to_index
from os import PathLike
from types import ModuleType, TracebackType
from typing import Any, Protocol, SupportsIndex, TypeAlias, overload

_Number: TypeAlias = int | float
_PageBox: TypeAlias = tuple[_Number, ...]
_PageGeometry: TypeAlias = tuple[_PageBox, _PageBox, float, bool]
_Table: TypeAlias = list[list[str | None]]
_Tables: TypeAlias = list[_Table]
_StreamItem: TypeAlias = tuple[int, _Tables]
_StreamFactory: TypeAlias = Callable[[], Iterable[_StreamItem]]
_SliceIndex: TypeAlias = slice
_OutFile: TypeAlias = str | bytes | int | PathLike[str] | PathLike[bytes]


class _DocLike(Protocol):
    def page_mediaboxes(self) -> Sequence[Sequence[_Number]]: ...

    def page_count(self) -> int: ...

    def get_page(self, page_index: int) -> object: ...


class _PdfLike(Protocol):
    doc: _DocLike | None
    pages_to_parse: Iterable[int] | None


class _PageLike(Protocol):
    bbox: Sequence[_Number]
    mediabox: Sequence[_Number]
    initial_doctop: _Number
    page_obj: object
    page_number: int
    objects: dict[str, Any]
    pdf: _PdfLike | None


class _HasPageNumber(Protocol):
    page_number: int


class _Closable(Protocol):
    def close(self) -> None: ...


class _StreamLike(Protocol):
    def close(self) -> None: ...


class _PdfClosable(Protocol):
    stream_is_external: bool
    stream: _StreamLike

    def flush_cache(self) -> None: ...

    def close(self) -> None: ...


def _set_attr(target: object, name: str, value: object) -> None:
    setattr(target, name, value)


def _mark_patched(func: Callable[..., object]) -> None:
    _set_attr(func, "_bolivar_patched", True)


def _module_from_sys(name: str) -> ModuleType | None:
    module = sys.modules.get(name)
    return module if isinstance(module, ModuleType) else None


def _apply_patch(module: ModuleType) -> bool:
    page_mod: object | None = getattr(module, "page", None)
    if page_mod is None and getattr(module, "__name__", "") == "pdfplumber.page":
        page_mod = module
        pkg = _module_from_sys("pdfplumber")
        if pkg is not None and not hasattr(pkg, "page"):
            _set_attr(pkg, "page", module)
            module = pkg
    if page_mod is None and getattr(module, "__name__", "") == "pdfplumber.pdf":
        page_mod = _module_from_sys("pdfplumber.page")
        pkg = _module_from_sys("pdfplumber")
        if pkg is not None and page_mod is not None and not hasattr(pkg, "page"):
            _set_attr(pkg, "page", page_mod)

    if page_mod is None:
        return False

    already_patched = getattr(page_mod.Page.extract_tables, "_bolivar_patched", False)

    from bolivar import extract_tables_stream_from_document
    from bolivar._native_api import _extract_tables_from_page_objects
    from pdfplumber.utils.exceptions import PdfminerException

    def _page_geom(page: _PageLike) -> _PageGeometry:
        return (
            tuple(page.bbox),
            tuple(page.mediabox),
            float(page.initial_doctop),
            not getattr(page, "is_original", True),
        )

    def _safe_page_mediaboxes(doc: _DocLike) -> Sequence[Sequence[_Number]]:
        try:
            return doc.page_mediaboxes()
        except PdfminerException:
            raise
        except Exception as e:
            raise PdfminerException(str(e)) from e

    def _base_geometries(doc: _DocLike) -> list[_PageGeometry]:
        boxes = _safe_page_mediaboxes(doc)
        doctops: list[float] = []
        running = 0.0
        for box in boxes:
            doctops.append(running)
            running += box[3] - box[1]
        return [
            (tuple(box), tuple(box), doctop, False)
            for box, doctop in zip(boxes, doctops, strict=False)
        ]

    def _get_base_geometries(
        pdf: object | None,
        doc: _DocLike,
    ) -> list[_PageGeometry]:
        if pdf is not None:
            base: list[_PageGeometry] | None = getattr(
                pdf, "_bolivar_table_geom_base", None
            )
            if base is not None:
                return base
        base = _base_geometries(doc)
        if pdf is not None:
            _set_attr(pdf, "_bolivar_table_geom_base", base)
        return base

    def _build_geometries(
        doc: _DocLike,
        page_index: int,
        page: _PageLike,
        base: list[_PageGeometry] | None = None,
    ) -> list[_PageGeometry]:
        if base is None:
            base = _base_geometries(doc)
        geoms = list(base)
        current = _page_geom(page)
        if 0 <= page_index < len(geoms) and geoms[page_index] != current:
            geoms[page_index] = current
        return geoms

    def _normalize_key(value: object) -> object:
        if isinstance(value, dict):
            return tuple(
                sorted((key, _normalize_key(val)) for key, val in value.items())
            )
        if isinstance(value, (list, tuple)):
            return tuple(_normalize_key(val) for val in value)
        return value

    def _settings_key(table_settings: object) -> object:
        return _normalize_key(table_settings)

    def _laparams_key(pdf: object | None) -> object:
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
        def __init__(
            self,
            stream_factory: _StreamFactory,
            cache_limit: int = _TABLE_STREAM_CACHE_LIMIT,
        ) -> None:
            self._stream_factory = stream_factory
            self._stream = iter(stream_factory())
            self._cache: dict[int, _Tables] = {}
            self._done = False
            self._lock = threading.Lock()
            self._max_index_seen = -1
            self._cache_limit = max(int(cache_limit), 0)

        def _evict_cache(self, newest_index: int) -> None:
            if self._cache_limit <= 0:
                self._cache.clear()
                return
            keep_from = newest_index - (self._cache_limit - 1)
            for key in list(self._cache.keys()):
                if key < keep_from:
                    self._cache.pop(key, None)

        def _get_from_fresh_stream(self, page_index: int) -> _Tables | None:
            stream = iter(self._stream_factory())
            for idx, tables in stream:
                if idx == page_index:
                    return tables
                if idx > page_index:
                    break
            return None

        def get(self, page_index: int) -> _Tables | None:
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

    def _get_table_stream(
        pdf: object | None,
        doc: _DocLike,
        table_settings: dict[str, Any] | None,
        geometries: Sequence[_PageGeometry],
        page_numbers: Sequence[int] | None,
    ) -> _BolivarTableStream:
        streams: dict[tuple[object, ...], _BolivarTableStream] | None = None
        if pdf is not None:
            streams = getattr(pdf, "_bolivar_table_streams", None)
            if streams is None:
                streams = {}
                _set_attr(pdf, "_bolivar_table_streams", streams)
        settings_key = _settings_key(table_settings)
        geometries_key = tuple(geometries)
        laparams_key = _laparams_key(pdf)
        page_numbers_key = tuple(page_numbers) if page_numbers is not None else None
        key = (settings_key, geometries_key, laparams_key, page_numbers_key)
        if streams is not None and key in streams:
            return streams[key]
        rust_doc = getattr(doc, "_rust_doc", None) or doc

        def _stream_factory() -> Iterable[_StreamItem]:
            return extract_tables_stream_from_document(
                rust_doc,  # type: ignore[arg-type]
                geometries,
                table_settings=table_settings,
                laparams=(getattr(pdf, "laparams", None) if pdf is not None else None),
                page_numbers=page_numbers,
                maxpages=0,
                caching=getattr(doc, "caching", True),
            )

        wrapped = _BolivarTableStream(_stream_factory)
        if streams is not None:
            streams[key] = wrapped
        return wrapped

    if not already_patched:

        def extract_tables_from_page(
            page: _PageLike,
            table_settings: dict[str, Any] | None = None,
        ) -> _Tables:
            if not getattr(page, "is_original", True):
                return _extract_tables_from_page_objects(
                    page.objects,
                    page.bbox,
                    page.mediabox,
                    page.initial_doctop,
                    table_settings=table_settings,
                    force_crop=not getattr(page, "is_original", True),
                )
            page_index = getattr(page.page_obj, "_page_index", page.page_number - 1)
            pdf = page.pdf
            doc: _DocLike | None = pdf.doc if pdf else None
            if doc is None:
                doc = getattr(page.page_obj, "doc", None)
            if doc is None:
                raise PdfminerException("pdf document missing")
            base_geoms = _get_base_geometries(pdf, doc)
            geoms = _build_geometries(doc, page_index, page, base=base_geoms)
            pages = getattr(pdf, "pages", None) if pdf is not None else None
            page_numbers = (
                list(getattr(pages, "_page_numbers", [])) if pages is not None else None
            )
            stream = _get_table_stream(pdf, doc, table_settings, geoms, page_numbers)
            tables = stream.get(page_index)
            return tables or []

        def _extract_tables(
            self: _PageLike, table_settings: dict[str, Any] | None = None
        ) -> _Tables:
            return extract_tables_from_page(self, table_settings)

        def _table_cell_count(table: _Table) -> int:
            return sum(len(row) for row in table)

        def _extract_table(
            self: _PageLike,
            table_settings: dict[str, Any] | None = None,
        ) -> _Table | None:
            tables = _extract_tables(self, table_settings=table_settings)
            if not tables:
                return None
            return max(tables, key=_table_cell_count)

        _mark_patched(_extract_tables)
        _set_attr(page_mod.Page, "extract_tables", _extract_tables)
        _mark_patched(_extract_table)
        _set_attr(page_mod.Page, "extract_table", _extract_table)

    class BolivarLazyPages(list[object]):
        def __init__(self, pdf: _PdfLike) -> None:
            self._pdf = pdf
            doc = pdf.doc if pdf is not None else None
            if doc is None:
                raise RuntimeError("pdf document missing")
            self._doc: _DocLike = doc
            page_count = self._doc.page_count()
            if page_count <= 0:
                raise PdfminerException("PDF contains no pages")
            pages_to_parse = pdf.pages_to_parse
            if pages_to_parse is None:
                self._page_numbers = list(range(page_count))
            else:
                allowed = set(pages_to_parse)
                self._page_numbers = [
                    idx for idx in range(page_count) if (idx + 1) in allowed
                ]
            self._page_number_set = set(self._page_numbers)
            self._page_cache: dict[int, object] = {}
            self._doctops: list[float] | None = None

        def close(self) -> None:
            for page in self._page_cache.values():
                close_fn = getattr(page, "close", None)
                if callable(close_fn):
                    close_fn()
            self._page_cache.clear()

        def _ensure_doctops(self) -> None:
            if self._doctops is not None:
                return
            boxes = _safe_page_mediaboxes(self._doc)
            doctops: list[float] = []
            running = 0.0
            try:
                for page_index in self._page_numbers:
                    box = boxes[page_index]
                    height = box[3] - box[1]
                    doctops.append(running)
                    running += height
            except IndexError as e:
                raise PdfminerException(str(e)) from e
            self._doctops = doctops

        def __len__(self) -> int:
            return len(self._page_numbers)

        @overload
        def __getitem__(self, idx: _SliceIndex) -> list[object]: ...

        @overload
        def __getitem__(self, idx: SupportsIndex) -> object: ...

        def __getitem__(
            self,
            idx: SupportsIndex | _SliceIndex,
        ) -> object | list[object]:
            if isinstance(idx, slice):
                return [self[i] for i in range(*idx.indices(len(self)))]
            try:
                idx = to_index(idx)
            except TypeError as e:
                raise TypeError("page index must be int or slice") from e
            if idx < 0:
                idx += len(self)
            if idx < 0 or idx >= len(self):
                raise IndexError("page index out of range")
            self._ensure_doctops()
            page_index = self._page_numbers[idx]
            cached = self._page_cache.get(page_index)
            if cached is not None:
                return cached
            assert self._doctops is not None
            doctops = self._doctops
            doctop = doctops[idx]
            try:
                page_obj = self._doc.get_page(page_index)
            except PdfminerException:
                raise
            except Exception as e:
                raise PdfminerException(str(e)) from e
            page = page_mod.Page(
                self._pdf,
                page_obj,
                page_number=page_index + 1,
                initial_doctop=doctop,
            )
            self._page_cache[page_index] = page
            return page

        def __iter__(self) -> Iterator[object]:
            for i in range(len(self)):
                yield self[i]

        def __reversed__(self) -> Iterator[object]:
            for i in range(len(self) - 1, -1, -1):
                yield self[i]

        def __contains__(self, item: object) -> bool:
            page_number = getattr(item, "page_number", None)
            if page_number is not None:
                return (page_number - 1) in self._page_number_set
            return False

        def __aiter__(self) -> AsyncIterator[object]:
            async def gen() -> AsyncIterator[object]:
                # Keep async iteration lightweight.
                # Avoid eager layout extraction to cap memory.
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
                            raise PdfminerException(str(e)) from e
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
    pdf_mod: ModuleType | None
    try:
        import pdfplumber.pdf as _pdf_mod
    except Exception:
        pdf_mod = None
    else:
        pdf_mod = _pdf_mod

    if pdf_mod is not None and hasattr(pdf_mod, "PDF"):
        pdf_cls = pdf_mod.PDF
        current_pages = getattr(pdf_cls, "pages", None)
        current_getter = (
            current_pages.fget if isinstance(current_pages, property) else None
        )
        if current_getter is None or not getattr(
            current_getter, "_bolivar_patched", False
        ):

            def _bolivar_pages(self: _PdfLike) -> BolivarLazyPages:
                pages = getattr(self, "_pages", None)
                if isinstance(pages, BolivarLazyPages):
                    return pages
                pages = BolivarLazyPages(self)
                _set_attr(self, "_pages", pages)
                return pages

            _mark_patched(_bolivar_pages)
            _set_attr(pdf_cls, "pages", property(_bolivar_pages))

        current_close = getattr(pdf_cls, "close", None)
        if current_close is None or not getattr(
            current_close, "_bolivar_patched", False
        ):

            def _bolivar_close(self: _PdfClosable) -> None:
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

            _mark_patched(_bolivar_close)
            _set_attr(pdf_cls, "close", _bolivar_close)

        if not hasattr(pdf_cls, "__aenter__"):

            async def _aenter(self: _PdfClosable) -> _PdfClosable:
                return self

            async def _aexit(
                self: _PdfClosable,
                exc_type: type[BaseException] | None,
                exc: BaseException | None,
                tb: TracebackType | None,
            ) -> bool:
                del exc_type, exc, tb
                self.close()
                return False

            _set_attr(pdf_cls, "__aenter__", _aenter)
            _set_attr(pdf_cls, "__aexit__", _aexit)

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
    repair_mod: ModuleType | None
    try:
        import pdfplumber.repair as _repair_mod
    except Exception:
        repair_mod = None
    else:
        repair_mod = _repair_mod

    if repair_mod is not None:
        from bolivar import repair_pdf

        def _rust_repair(
            path_or_fp: bytes | bytearray,
            password: str | None = None,
            gs_path: object = None,
            setting: str = "default",
        ) -> BytesIO:
            del password, gs_path, setting
            return BytesIO(repair_pdf(path_or_fp))

        def _rust_repair_public(
            path_or_fp: object,
            outfile: _OutFile | None = None,
            password: str | None = None,
            gs_path: object = None,
            setting: str = "default",
        ) -> BytesIO | None:
            repaired = _rust_repair(
                path_or_fp,  # type: ignore[arg-type]
                password=password,
                gs_path=gs_path,
                setting=setting,
            )
            if outfile is not None:
                with open(outfile, "wb") as f:
                    f.write(repaired.read())
                return None
            return repaired

        _set_attr(repair_mod, "_repair", _rust_repair)
        _set_attr(repair_mod, "repair", _rust_repair_public)
        if hasattr(module, "repair"):
            _set_attr(module, "repair", _rust_repair_public)
        pdf_mod = _module_from_sys("pdfplumber.pdf")
        if pdf_mod is not None and hasattr(pdf_mod, "_repair"):
            _set_attr(pdf_mod, "_repair", _rust_repair)

    # Only consider patch complete when PDF.pages is patched
    return pdf_pages_patched


_HOOK_INSTALLED = False
_HOOK_LOCK = threading.Lock()
_PATCH_APPLIED = False
_logger = logging.getLogger("bolivar.pdfplumber_patch")


class _PdfplumberPatchLoader(importlib.abc.Loader):
    def __init__(self, loader: importlib.abc.Loader) -> None:
        self.loader = loader

    def create_module(
        self,
        spec: importlib.machinery.ModuleSpec,
    ) -> ModuleType | None:
        if hasattr(self.loader, "create_module"):
            return self.loader.create_module(spec)
        return None

    def exec_module(self, module: ModuleType) -> None:
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
    def __init__(self, names: Iterable[str]) -> None:
        self.names = set(names)

    def find_spec(
        self,
        fullname: str,
        path: Sequence[str] | None,
        target: ModuleType | None = None,
    ) -> importlib.machinery.ModuleSpec | None:
        del target
        if fullname not in self.names:
            return None
        spec = importlib.machinery.PathFinder.find_spec(fullname, path)
        if spec is None or spec.loader is None:
            return spec
        spec.loader = _PdfplumberPatchLoader(spec.loader)
        return spec


def _install_hook(names: set[str] | None = None) -> None:
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


def _remove_hook_unlocked() -> None:
    global _HOOK_INSTALLED
    if not _HOOK_INSTALLED:
        return
    sys.meta_path = [
        m for m in sys.meta_path if not isinstance(m, _PdfplumberPatchFinder)
    ]
    _HOOK_INSTALLED = False


def _remove_hook() -> None:
    with _HOOK_LOCK:
        _remove_hook_unlocked()


def patch_pdfplumber() -> bool:
    module = _module_from_sys("pdfplumber")
    if module is not None and hasattr(module, "page"):
        return _apply_patch(module)

    if module is not None:
        _install_hook({"pdfplumber.page", "pdfplumber.pdf"})
        return False

    _install_hook()
    return False
