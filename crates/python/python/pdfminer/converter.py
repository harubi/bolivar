# pdfminer.converter compatibility shim
#
# PDFPageAggregator is kept in pure Python for subclassability.
# pdfplumber subclasses it as PDFPageAggregatorWithMarkedContent.

import io
import re
from collections.abc import Sequence
from typing import Protocol, TypeAlias

from bolivar._native_api import (
    HOCRConverter as _NativeHOCRConverter,
)
from bolivar._native_api import (
    HTMLConverter as _NativeHTMLConverter,
)
from bolivar._native_api import (
    TextConverter as _NativeTextConverter,
)
from bolivar._native_api import (
    XMLConverter as _NativeXMLConverter,
)

from .layout import LTCurve, LTLine, LTPage, LTRect
from .utils import MATRIX_IDENTITY, apply_matrix_pt


class _LayoutItemSink(Protocol):
    def add(self, item: object) -> None:
        """Add a layout item to the current container."""


_DashingStyle: TypeAlias = tuple[list[float], float] | None


class _GraphicState(Protocol):
    linewidth: float
    scolor: object
    ncolor: object
    dash: _DashingStyle


_PathOperand = str | float | int
_PathOperation = Sequence[_PathOperand]


class PDFPageAggregator:
    """Collects layout items from a PDF page.

    Pure Python implementation for subclassability.
    pdfplumber subclasses this as PDFPageAggregatorWithMarkedContent.
    """

    _laparams: object | None

    def __init__(
        self,
        rsrcmgr: object,
        pageno: int = 1,
        laparams: object | None = None,
    ) -> None:
        self.rsrcmgr = rsrcmgr
        self.pageno = pageno
        # Store laparams for interpreter to access
        self._laparams = laparams
        self.laparams = laparams
        self.cur_item: object | None = None

    def begin_page(self, page: object, ctm: Sequence[float]) -> None:
        """Called at the start of page processing."""
        pass

    def end_page(self, page: object) -> None:
        """Called at the end of page processing."""
        pass

    def begin_figure(
        self,
        name: str,
        bbox: object,
        matrix: Sequence[float],
    ) -> None:
        """Called when entering a figure."""
        pass

    def end_figure(self, name: str) -> None:
        """Called when exiting a figure."""
        pass

    def receive_layout(self, ltpage: object) -> None:
        """Receive the analyzed layout for a page (public API)."""
        self.cur_item = ltpage

    def _receive_layout(self, rust_ltpage: object) -> None:
        """Receive layout from Rust (internal API).

        Receives the Rust LTPage directly.
        """
        self.receive_layout(LTPage(rust_ltpage))

    def get_result(self) -> object | None:
        """Get the current page's layout result."""
        return self.cur_item


class PDFLayoutAnalyzer:
    """PDFLayoutAnalyzer - minimal paint_path implementation for tests."""

    def __init__(
        self,
        rsrcmgr: object,
        pageno: int = 1,
        laparams: object | None = None,
    ) -> None:
        self.rsrcmgr = rsrcmgr
        self.pageno = pageno
        self.laparams = laparams
        self.cur_item: _LayoutItemSink | None = None
        self.ctm = MATRIX_IDENTITY
        self._stack = []

    def set_ctm(self, ctm: Sequence[float]) -> None:
        a, b, c, d, e, f = ctm
        self.ctm = (a, b, c, d, e, f)

    def paint_path(
        self,
        gstate: _GraphicState,
        stroke: bool,
        fill: bool,
        evenodd: bool,
        path: Sequence[_PathOperation],
    ) -> None:
        """Paint paths described in section 4.4 of the PDF reference manual."""
        if not path:
            return
        shape = "".join(str(x[0]) for x in path)

        if shape[:1] != "m":
            return
        if shape.count("m") > 1:
            for m in re.finditer(r"m[^m]+", shape):
                subpath = path[m.start(0) : m.end(0)]
                self.paint_path(gstate, stroke, fill, evenodd, subpath)
            return

        # Points for each operation (h uses starting point).
        raw_pts = [
            (float(p[-2]), float(p[-1]))
            if p[0] != "h"
            else (float(path[0][-2]), float(path[0][-1]))
            for p in path
        ]
        pts = [apply_matrix_pt(self.ctm, pt) for pt in raw_pts]

        operators = [str(operation[0]) for operation in path]
        transformed_points = [
            [
                apply_matrix_pt(self.ctm, (float(a), float(b)))
                for a, b in zip(operation[1::2], operation[2::2], strict=False)
            ]
            for operation in path
        ]
        transformed_path = [
            (o, *p) for o, p in zip(operators, transformed_points, strict=False)
        ]

        # Drop redundant "l" on a path closed with "h".
        if len(shape) > 3 and shape[-2:] == "lh" and pts[-2] == pts[0]:
            shape = shape[:-2] + "h"
            pts.pop()

        if shape in {"mlh", "ml"}:
            line = LTLine(
                gstate.linewidth,
                [pts[0], pts[1]],
                stroke,
                fill,
                evenodd,
                gstate.scolor,
                gstate.ncolor,
                gstate.dash,
            )
            line.original_path = transformed_path
            line.dashing_style = gstate.dash
            if self.cur_item is not None:
                self.cur_item.add(line)
            return

        if shape in {"mlllh", "mllll"}:
            (x0, y0), (x1, y1), (x2, y2), (x3, y3), _ = pts
            is_closed_loop = pts[0] == pts[4]
            has_square_coordinates = (
                x0 == x1 and y1 == y2 and x2 == x3 and y3 == y0
            ) or (y0 == y1 and x1 == x2 and y2 == y3 and x3 == x0)
            if is_closed_loop and has_square_coordinates:
                rect = LTRect(
                    gstate.linewidth,
                    [pts[0], pts[1], pts[2], pts[3]],
                    stroke,
                    fill,
                    evenodd,
                    gstate.scolor,
                    gstate.ncolor,
                    gstate.dash,
                )
                rect.original_path = transformed_path
                rect.dashing_style = gstate.dash
                if self.cur_item is not None:
                    self.cur_item.add(rect)
                return

        curve = LTCurve(
            gstate.linewidth,
            pts,
            stroke,
            fill,
            evenodd,
            gstate.scolor,
            gstate.ncolor,
            gstate.dash,
        )
        curve.original_path = transformed_path
        curve.dashing_style = gstate.dash
        if self.cur_item is not None:
            self.cur_item.add(curve)


class PDFConverter(PDFLayoutAnalyzer):
    """Base class for PDF converters."""

    def __init__(
        self,
        rsrcmgr: object,
        outfp: object,
        codec: str = "utf-8",
        pageno: int = 1,
        laparams: object | None = None,
    ) -> None:
        super().__init__(rsrcmgr, pageno=pageno, laparams=laparams)
        self.outfp = outfp
        self.codec = codec
        self.outfp_binary = self._is_binary_stream(self.outfp)

    @staticmethod
    def _is_binary_stream(outfp: object) -> bool:
        if "b" in getattr(outfp, "mode", ""):
            return True
        if hasattr(outfp, "mode"):
            return False
        if isinstance(outfp, io.BytesIO):
            return True
        return not isinstance(outfp, (io.StringIO, io.TextIOBase))


# Canonical runtime converter exports.
TextConverter = _NativeTextConverter
HTMLConverter = _NativeHTMLConverter
HOCRConverter = _NativeHOCRConverter
XMLConverter = _NativeXMLConverter
