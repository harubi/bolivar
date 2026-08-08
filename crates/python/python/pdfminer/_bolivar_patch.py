import importlib.abc
import importlib.machinery
import logging
import sys
import threading
from collections.abc import AsyncIterator, Callable, Iterable, Iterator, Sequence
from io import BufferedReader, BytesIO
from operator import index as to_index
from os import PathLike, fspath
from types import ModuleType, TracebackType
from typing import (
    TYPE_CHECKING,
    Any,
    Protocol,
    SupportsIndex,
    TypeAlias,
    cast,
    overload,
)

_Number: TypeAlias = int | float
_PageBox: TypeAlias = tuple[_Number, ...]
_PageGeometry: TypeAlias = tuple[_PageBox, _PageBox, float, bool]
_Table: TypeAlias = list[list[str | None]]
_Tables: TypeAlias = list[_Table]
_Word: TypeAlias = dict[str, Any]
_Words: TypeAlias = list[_Word]
_SliceIndex: TypeAlias = slice
_OutFile: TypeAlias = str | bytes | int | PathLike[str] | PathLike[bytes]
_RepairInput: TypeAlias = (
    str | PathLike[str] | PathLike[bytes] | BufferedReader | BytesIO | bytes | bytearray
)

if TYPE_CHECKING:
    from bolivar._native_api import PDFDocument as _NativePDFDocument


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
    chars: list[dict[str, Any]]
    lines: list[dict[str, Any]]
    rects: list[dict[str, Any]]
    curves: list[dict[str, Any]]
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

    from bolivar._bridge_api import (
        _extract_tables_for_compat_page,
        _extract_tables_for_page_indexed,
        _extract_words_for_page_indexed,
    )
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

    def _can_use_rust_words(kwargs: dict[str, Any]) -> bool:
        if kwargs.get("return_chars"):
            return False
        extra_attrs = kwargs.get("extra_attrs")
        if extra_attrs not in (None, []):
            return False
        allowed = {"return_chars", "extra_attrs"}
        return not any(key not in allowed for key in kwargs)

    def _extract_words_for_page(
        page: _PageLike,
        pdf: object | None,
        doc: _DocLike,
        page_index: int,
        text_settings: dict[str, Any] | None,
    ) -> _Words:
        rust_doc = getattr(doc, "_rust_doc", None) or doc
        native_doc = cast("_NativePDFDocument", rust_doc)
        words = _extract_words_for_page_indexed(
            native_doc,
            page_index,
            _page_geom(page),
            text_settings=text_settings,
            laparams=getattr(pdf, "laparams", None) if pdf is not None else None,
            caching=getattr(doc, "caching", True),
        )
        if words is not None:
            return words
        raise RuntimeError(f"missing words for page {page.page_number}")

    if not already_patched:
        _orig_extract_words = page_mod.Page.extract_words

        def extract_tables_from_page(
            page: _PageLike,
            table_settings: dict[str, Any] | None = None,
        ) -> _Tables:
            if not getattr(page, "is_original", True):
                return _extract_tables_for_compat_page(
                    page.chars,
                    page.lines,
                    page.rects,
                    page.curves,
                    _page_geom(page),
                    table_settings=table_settings,
                )
            page_index = getattr(page.page_obj, "_page_index", page.page_number - 1)
            pdf = page.pdf
            doc: _DocLike | None = pdf.doc if pdf else None
            if doc is None:
                doc = getattr(page.page_obj, "doc", None)
            if doc is None:
                raise PdfminerException("pdf document missing")
            rust_doc = getattr(doc, "_rust_doc", None) or doc
            native_doc = cast("_NativePDFDocument", rust_doc)
            return _extract_tables_for_page_indexed(
                native_doc,
                page_index,
                _page_geom(page),
                table_settings=table_settings,
                laparams=(getattr(pdf, "laparams", None) if pdf is not None else None),
                caching=getattr(doc, "caching", True),
            )

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

        def _extract_words(self: _PageLike, **kwargs: object) -> _Words:
            if not getattr(self, "is_original", True):
                return cast("_Words", _orig_extract_words(self, **kwargs))
            if not hasattr(self, "page_obj") or not hasattr(self, "page_number"):
                return cast("_Words", _orig_extract_words(self, **kwargs))
            word_kwargs = cast("dict[str, Any]", dict(kwargs))
            if not _can_use_rust_words(word_kwargs):
                return cast("_Words", _orig_extract_words(self, **kwargs))
            word_kwargs.pop("return_chars", None)
            word_kwargs.pop("extra_attrs", None)
            page_index = getattr(self.page_obj, "_page_index", self.page_number - 1)
            pdf = self.pdf
            doc: _DocLike | None = pdf.doc if pdf else None
            if doc is None:
                doc = getattr(self.page_obj, "doc", None)
            if doc is None:
                return cast("_Words", _orig_extract_words(self, **kwargs))
            return _extract_words_for_page(
                self, pdf, doc, page_index, word_kwargs or None
            )

        _mark_patched(_extract_words)
        _set_attr(page_mod.Page, "extract_words", _extract_words)

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
            return any(page == item for page in self)

        def __repr__(self) -> str:
            return repr(list(self))

        def copy(self) -> list[object]:
            return list(self)

        def index(
            self,
            item: object,
            start: SupportsIndex = 0,
            stop: SupportsIndex = sys.maxsize,
        ) -> int:
            start_index, stop_index, _ = slice(start, stop).indices(len(self))
            for position in range(start_index, stop_index):
                if self[position] == item:
                    return position
            raise ValueError(f"{item!r} is not in list")

        def count(self, item: object) -> int:
            return sum(1 for page in self if page == item)

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
            path_or_fp: _RepairInput,
            password: str | None = None,
            gs_path: object = None,
            setting: str = "default",
        ) -> BytesIO:
            del password, gs_path, setting
            payload: bytes | bytearray
            if isinstance(path_or_fp, (bytes, bytearray)):
                payload = path_or_fp
            elif isinstance(path_or_fp, (str, PathLike)):
                with open(fspath(path_or_fp), "rb") as f:
                    payload = f.read()
            else:
                payload = path_or_fp.read()
            return BytesIO(repair_pdf(payload))

        def _rust_repair_public(
            path_or_fp: _RepairInput,
            outfile: _OutFile | None = None,
            password: str | None = None,
            gs_path: object = None,
            setting: str = "default",
        ) -> BytesIO | None:
            repaired = _rust_repair(
                path_or_fp,
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
