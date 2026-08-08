# pdfminer.pdfinterp compatibility shim
from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Any, Protocol, cast

if TYPE_CHECKING:
    from bolivar._bolivar import PDFDocument as _NativePDFDocument
    from bolivar._bolivar import PDFPage as _NativePDFPage

    from .pdfpage import PDFPage

from bolivar import (
    process_page as _rust_process_page,
)
from bolivar._native_api import PDFResourceManager as _RustPDFResourceManager

from .layout import PDFGraphicState
from .psparser import PSLiteral, literal_name

log: logging.Logger = logging.getLogger(__name__)

# PDFStackT type alias (matches pdfminer.six)
PDFStackT = (
    bool | int | float | bytes | str | list[Any] | dict[str, Any] | tuple[Any, ...]
)
PatternColor = str | tuple[float | tuple[float, ...], str]
_Matrix = tuple[float, float, float, float, float, float]


# Rust-backed resource manager (drop-in compatible API).
PDFResourceManager = _RustPDFResourceManager


class _DeviceLike(Protocol):
    _laparams: object | None

    def _receive_layout(self, layout: object) -> None: ...

    def _process_page(
        self,
        doc: _NativePDFDocument,
        page: _NativePDFPage,
        rotate: int | None = None,
        caching: bool = True,
    ) -> None: ...


class PDFColorSpace:
    name: str
    ncomponents: int

    def __init__(self, name: str = "DeviceGray", ncomponents: int = 1) -> None:
        self.name = name
        self.ncomponents = ncomponents


class PDFPageInterpreter:
    """PDF page interpreter - wraps bolivar's Rust page processing.

    Provides pdfminer.six-compatible API for page interpretation.
    """

    def __init__(self, rsrcmgr: PDFResourceManager, device: object) -> None:
        """Create a page interpreter.

        Args:
            rsrcmgr: PDFResourceManager instance (for compatibility)
            device: PDFDevice instance (PDFPageAggregator or similar)
        """
        self.rsrcmgr = rsrcmgr
        self.device: _DeviceLike = cast("_DeviceLike", device)
        self._stack: list[PDFStackT] = []
        self.graphicstate: PDFGraphicState | None = None
        self.ctm: object | None = None

    def _get_graphicstate(self) -> PDFGraphicState:
        if self.graphicstate is None:
            self.graphicstate = PDFGraphicState()
        if self.graphicstate.scs is None:
            self.graphicstate.scs = PDFColorSpace()
        if self.graphicstate.ncs is None:
            self.graphicstate.ncs = PDFColorSpace()
        return self.graphicstate

    def init_resources(self, resources: object) -> None:
        self.resources = resources
        self.graphicstate = PDFGraphicState()
        self._get_graphicstate()

    def init_state(self, ctm: object) -> None:
        self.ctm = ctm
        set_ctm = getattr(self.device, "set_ctm", None)
        if callable(set_ctm):
            set_ctm(ctm)

    def push(self, obj: PDFStackT) -> None:
        self._stack.append(obj)

    def pop(self, n: int) -> list[PDFStackT]:
        return self._popn(n)

    def dup(self) -> PDFPageInterpreter:
        return self.__class__(self.rsrcmgr, self.device)

    def _popn(self, n: int) -> list[PDFStackT]:
        if n <= 0:
            return []
        vals = self._stack[-n:]
        del self._stack[-n:]
        return vals

    def _pattern_color(self, is_stroking: bool, iso_ref: str) -> PatternColor | None:
        state = self._get_graphicstate()
        cs = state.scs if is_stroking else state.ncs
        ncomponents = getattr(cs, "ncomponents", 1) or 1
        if ncomponents <= 1:
            name = self._popn(1)[0] if self._stack else None
            if not isinstance(name, PSLiteral):
                log.warning(
                    (
                        "Pattern color space requires name object (PSLiteral); "
                        "got %s: %s (ISO 32000 %s)"
                    ),
                    type(name).__name__,
                    name,
                    iso_ref,
                )
                return None
            return literal_name(name)
        values = self._popn(ncomponents)
        if len(values) != ncomponents:
            return None
        pattern = values[-1]
        if not isinstance(pattern, PSLiteral):
            log.warning(
                (
                    "Pattern color space requires name object (PSLiteral); "
                    "got %s: %s (ISO 32000 %s)"
                ),
                type(pattern).__name__,
                pattern,
                iso_ref,
            )
            return None
        base_vals_list: list[float] = []
        for component in values[:-1]:
            if not isinstance(component, (int, float)):
                return None
            base_vals_list.append(float(component))
        base_vals = tuple(base_vals_list)
        base_color = base_vals[0] if len(base_vals) == 1 else base_vals
        return (base_color, literal_name(pattern))

    def do_SCN(self) -> None:
        state = self._get_graphicstate()
        if getattr(state.scs, "name", None) == "Pattern":
            color = self._pattern_color(True, "8.7.3.3")
            if color is not None:
                state.scolor = color

    def do_scn(self) -> None:
        state = self._get_graphicstate()
        if getattr(state.ncs, "name", None) == "Pattern":
            color = self._pattern_color(False, "8.7.3.2")
            if color is not None:
                state.ncolor = color

    def process_page(self, page: PDFPage) -> None:
        """Process a PDF page and send results to device.

        Args:
            page: PDFPage instance to process
        """
        # Get the Rust document and page from the shim wrappers
        rust_doc = page.doc._rust_doc
        rust_page = page._rust_page
        rotation = (page.rotate - rust_page.rotate) % 360

        # Get LAParams from the device if available
        laparams = getattr(self.device, "_laparams", None)
        rust_laparams = None
        if laparams is not None:
            # Convert to Rust LAParams if it's a shim LAParams
            if hasattr(laparams, "_to_rust"):
                rust_laparams = laparams._to_rust()
            else:
                # Already a Rust LAParams or compatible
                rust_laparams = laparams

        process_native_page = getattr(self.device, "_process_page", None)
        if callable(process_native_page):
            caching = True
            resource_caching = getattr(self.rsrcmgr, "caching", True)
            if callable(resource_caching):
                caching = bool(resource_caching())
            else:
                caching = bool(resource_caching)
            process_native_page(
                rust_doc,
                rust_page,
                rotate=getattr(page, "rotate", None),
                caching=caching,
            )
            return

        if hasattr(self.device, "_receive_layout"):
            ltpage = _rust_process_page(rust_doc, rust_page, rust_laparams, rotation)
            self.device._receive_layout(ltpage)
            return

        ctm = self._page_ctm(page)
        begin_page = getattr(self.device, "begin_page", None)
        if callable(begin_page):
            begin_page(page, ctm)
        self.init_resources(getattr(page, "resources", {}))
        self.init_state(ctm)
        end_page = getattr(self.device, "end_page", None)
        if callable(end_page):
            end_page(page)

    def render_contents(
        self,
        resources: object,
        streams: object,
        ctm: _Matrix = (1, 0, 0, 1, 0, 0),
    ) -> None:
        self.init_resources(resources)
        self.init_state(ctm)
        _ = streams

    @staticmethod
    def _page_ctm(page: PDFPage) -> _Matrix:
        mediabox = getattr(page, "mediabox", None)
        if not isinstance(mediabox, (list, tuple)) or len(mediabox) != 4:
            return (1, 0, 0, 1, 0, 0)
        x0, y0, x1, y1 = (float(value) for value in mediabox)
        if page.rotate == 90:
            return (0, -1, 1, 0, -y0, x1)
        if page.rotate == 180:
            return (-1, 0, 0, -1, x1, y1)
        if page.rotate == 270:
            return (0, 1, -1, 0, y1, -x0)
        return (1, 0, 0, 1, -x0, -y0)
