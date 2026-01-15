# pdfminer.converter compatibility shim
#
# PDFPageAggregator is kept in pure Python for subclassability.
# pdfplumber subclasses it as PDFPageAggregatorWithMarkedContent.

import io
import re

from .layout import LTCurve, LTLine, LTPage, LTRect
from .utils import MATRIX_IDENTITY, apply_matrix_pt


class PDFPageAggregator:
    """Collects layout items from a PDF page.

    Pure Python implementation for subclassability.
    pdfplumber subclasses this as PDFPageAggregatorWithMarkedContent.
    """

    def __init__(self, rsrcmgr, pageno=1, laparams=None):
        self.rsrcmgr = rsrcmgr
        self.pageno = pageno
        # Store laparams for interpreter to access
        self._laparams = laparams
        self.laparams = laparams
        self.cur_item = None

    def begin_page(self, page, ctm):
        """Called at the start of page processing."""
        pass

    def end_page(self, page):
        """Called at the end of page processing."""
        pass

    def begin_figure(self, name, bbox, matrix):
        """Called when entering a figure."""
        pass

    def end_figure(self, name):
        """Called when exiting a figure."""
        pass

    def receive_layout(self, ltpage):
        """Receive the analyzed layout for a page (public API)."""
        self.cur_item = ltpage

    def _receive_layout(self, rust_ltpage):
        """Receive layout from Rust (internal API).

        Receives the Rust LTPage directly.
        """
        self.receive_layout(rust_ltpage)

    def get_result(self):
        """Get the current page's layout result."""
        return self.cur_item


class PDFLayoutAnalyzer:
    """PDFLayoutAnalyzer - minimal paint_path implementation for tests."""

    def __init__(self, rsrcmgr, pageno=1, laparams=None):
        self.rsrcmgr = rsrcmgr
        self.pageno = pageno
        self.laparams = laparams
        self.cur_item = None
        self.ctm = MATRIX_IDENTITY
        self._stack = []

    def set_ctm(self, ctm):
        self.ctm = tuple(ctm)

    def paint_path(self, gstate, stroke, fill, evenodd, path):
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
        raw_pts = [tuple(p[-2:]) if p[0] != "h" else tuple(path[0][-2:]) for p in path]
        pts = [apply_matrix_pt(self.ctm, (float(x), float(y))) for x, y in raw_pts]

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

    def __init__(self, rsrcmgr, outfp, codec="utf-8", pageno=1, laparams=None):
        super().__init__(rsrcmgr, pageno=pageno, laparams=laparams)
        self.outfp = outfp
        self.codec = codec
        self.outfp_binary = self._is_binary_stream(self.outfp)

    @staticmethod
    def _is_binary_stream(outfp):
        if "b" in getattr(outfp, "mode", ""):
            return True
        if hasattr(outfp, "mode"):
            return False
        if isinstance(outfp, io.BytesIO):
            return True
        if isinstance(outfp, (io.StringIO, io.TextIOBase)):
            return False
        return True


class TextConverter:
    """Converts PDF to plain text - stub."""

    def __init__(
        self,
        rsrcmgr,
        outfp,
        codec="utf-8",
        pageno=1,
        laparams=None,
        showpageno=False,
        imagewriter=None,
    ):
        raise NotImplementedError("TextConverter not yet implemented via bolivar")


class HTMLConverter:
    """Converts PDF to HTML - stub."""

    def __init__(
        self,
        rsrcmgr,
        outfp,
        codec="utf-8",
        pageno=1,
        laparams=None,
        scale=1,
        fontscale=1.0,
        layoutmode="normal",
        showpageno=True,
        imagewriter=None,
    ):
        raise NotImplementedError("HTMLConverter not yet implemented via bolivar")


class XMLConverter:
    """Converts PDF to XML - stub."""

    def __init__(
        self,
        rsrcmgr,
        outfp,
        codec="utf-8",
        pageno=1,
        laparams=None,
        stripcontrol=False,
        imagewriter=None,
    ):
        raise NotImplementedError("XMLConverter not yet implemented via bolivar")


# Rust-backed converters (override stubs).
from bolivar._bolivar import (  # noqa: E402
    TextConverter as _RustTextConverter,
    HTMLConverter as _RustHTMLConverter,
    HOCRConverter as _RustHOCRConverter,
    XMLConverter as _RustXMLConverter,
)

TextConverter = _RustTextConverter
HTMLConverter = _RustHTMLConverter
HOCRConverter = _RustHOCRConverter
XMLConverter = _RustXMLConverter
