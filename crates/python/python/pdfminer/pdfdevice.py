# pdfminer.pdfdevice compatibility shim

from collections.abc import Sequence

from bolivar._native_api import TagExtractor

_Matrix = tuple[float, float, float, float, float, float]


class PDFDevice:
    def __init__(self, rsrcmgr: object | None = None) -> None:
        self.rsrcmgr = rsrcmgr
        self.ctm: _Matrix | None = None

    def __repr__(self) -> str:
        return "<PDFDevice>"

    def __enter__(self) -> "PDFDevice":
        return self

    def __exit__(self, exc_type: object, exc_val: object, exc_tb: object) -> None:
        self.close()

    def close(self) -> None:
        return None

    def set_ctm(self, ctm: Sequence[float]) -> None:
        a, b, c, d, e, f = ctm
        self.ctm = (a, b, c, d, e, f)

    def begin_tag(self, tag: object, props: object | None = None) -> None:
        return None

    def end_tag(self) -> None:
        return None

    def do_tag(self, tag: object, props: object | None = None) -> None:
        return None

    def begin_page(self, page: object, ctm: Sequence[float]) -> None:
        return None

    def end_page(self, page: object) -> None:
        return None

    def begin_figure(self, name: str, bbox: object, matrix: Sequence[float]) -> None:
        return None

    def end_figure(self, name: str) -> None:
        return None

    def paint_path(
        self,
        graphicstate: object,
        stroke: bool,
        fill: bool,
        evenodd: bool,
        path: Sequence[object],
    ) -> None:
        return None

    def render_image(self, name: str, stream: object) -> None:
        return None

    def render_string(
        self,
        textstate: object,
        seq: object,
        ncs: object,
        graphicstate: object,
    ) -> None:
        return None


__all__ = ["PDFDevice", "TagExtractor"]
