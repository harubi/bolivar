# pdfminer.layout compatibility shim
#
# Layout analysis types. Currently stubs - will be replaced with PyO3 bindings.

from bolivar._bolivar import LAParams
from typing import Iterator, List, Optional, Tuple, Any


class LTItem:
    """Base class for layout items.

    All layout objects have a bounding box (x0, y0, x1, y1).
    """

    def __init__(self, bbox: Tuple[float, float, float, float] = (0, 0, 0, 0)):
        self.x0, self.y0, self.x1, self.y1 = bbox

    @property
    def bbox(self) -> Tuple[float, float, float, float]:
        return (self.x0, self.y0, self.x1, self.y1)

    @property
    def width(self) -> float:
        return self.x1 - self.x0

    @property
    def height(self) -> float:
        return self.y1 - self.y0

    def __repr__(self):
        return f"<{self.__class__.__name__} {self.bbox}>"


class LTComponent(LTItem):
    """A layout component with set_bbox method."""

    def set_bbox(self, bbox: Tuple[float, float, float, float]):
        self.x0, self.y0, self.x1, self.y1 = bbox


class LTContainer(LTComponent):
    """A container that holds other layout items."""

    def __init__(self, bbox: Tuple[float, float, float, float] = (0, 0, 0, 0)):
        super().__init__(bbox)
        self._objs: List[LTItem] = []

    def __iter__(self) -> Iterator[LTItem]:
        return iter(self._objs)

    def __len__(self) -> int:
        return len(self._objs)

    def add(self, obj: LTItem):
        self._objs.append(obj)

    def extend(self, objs: List[LTItem]):
        self._objs.extend(objs)


class LTTextContainer(LTContainer):
    """A container for text items."""

    def get_text(self) -> str:
        """Get text content."""
        return "".join(
            obj.get_text() if hasattr(obj, "get_text") else ""
            for obj in self._objs
        )


class LTAnno(LTItem):
    """Virtual annotation (space, newline) inserted during analysis."""

    def __init__(self, text: str):
        super().__init__((0, 0, 0, 0))
        self._text = text

    def get_text(self) -> str:
        return self._text


class LTChar(LTComponent):
    """A single character with position and font info."""

    def __init__(
        self,
        matrix,
        font,
        fontsize: float,
        scaling: float,
        rise: float,
        text: str,
        textwidth: float,
        textdisp,
        ncs,
        graphicstate,
    ):
        # Calculate bbox from matrix and dimensions
        # Simplified - actual implementation uses matrix transforms
        self.matrix = matrix
        self.font = font
        self.fontsize = fontsize
        self.scaling = scaling
        self.rise = rise
        self._text = text
        self.textwidth = textwidth
        self.textdisp = textdisp
        self.ncs = ncs
        self.graphicstate = graphicstate

        # Simplified bbox calculation
        x0 = matrix[4]
        y0 = matrix[5]
        x1 = x0 + textwidth * fontsize
        y1 = y0 + fontsize
        super().__init__((x0, y0, x1, y1))

    def get_text(self) -> str:
        return self._text

    @property
    def fontname(self) -> str:
        if self.font:
            return self.font.fontname if hasattr(self.font, "fontname") else str(self.font)
        return ""

    @property
    def size(self) -> float:
        return self.fontsize


class LTCurve(LTComponent):
    """A curve (line, bezier) in the PDF."""

    def __init__(
        self,
        linewidth: float,
        pts: List[Tuple[float, float]],
        stroke: bool = False,
        fill: bool = False,
        evenodd: bool = False,
        stroking_color=None,
        non_stroking_color=None,
        dashing=None,
        ncs=None,
        scs=None,
    ):
        self.linewidth = linewidth
        self.pts = pts
        self.stroke = stroke
        self.fill = fill
        self.evenodd = evenodd
        self.stroking_color = stroking_color
        self.non_stroking_color = non_stroking_color
        self.dashing = dashing
        self.ncs = ncs
        self.scs = scs

        # Calculate bbox from points
        if pts:
            xs = [p[0] for p in pts]
            ys = [p[1] for p in pts]
            bbox = (min(xs), min(ys), max(xs), max(ys))
        else:
            bbox = (0, 0, 0, 0)
        super().__init__(bbox)


class LTLine(LTCurve):
    """A straight line."""
    pass


class LTRect(LTCurve):
    """A rectangle."""
    pass


class LTFigure(LTContainer):
    """A figure (Form XObject) in the PDF."""

    def __init__(self, name: str, bbox: Tuple[float, float, float, float], matrix):
        super().__init__(bbox)
        self.name = name
        self.matrix = matrix


class LTTextLine(LTTextContainer):
    """A line of text."""
    pass


class LTTextLineHorizontal(LTTextLine):
    """A horizontal line of text."""
    pass


class LTTextLineVertical(LTTextLine):
    """A vertical line of text."""
    pass


class LTTextBox(LTTextContainer):
    """A box containing multiple lines of text."""

    index: int = 0

    def __init__(self, bbox: Tuple[float, float, float, float] = (0, 0, 0, 0)):
        super().__init__(bbox)
        self.index = 0


class LTTextBoxHorizontal(LTTextBox):
    """A horizontal text box."""
    pass


class LTTextBoxVertical(LTTextBox):
    """A vertical text box."""
    pass


class LTImage(LTComponent):
    """An image in the PDF."""

    def __init__(self, name: str, stream, bbox: Tuple[float, float, float, float]):
        super().__init__(bbox)
        self.name = name
        self.stream = stream
        self.srcsize = (0, 0)
        self.imagemask = False
        self.bits = 8
        self.colorspace = None


class LTPage(LTContainer):
    """A page containing layout items."""

    def __init__(
        self,
        pageid: int,
        bbox: Tuple[float, float, float, float],
        rotate: int = 0,
    ):
        super().__init__(bbox)
        self.pageid = pageid
        self.rotate = rotate


__all__ = [
    "LAParams",
    "LTAnno",
    "LTChar",
    "LTComponent",
    "LTContainer",
    "LTCurve",
    "LTFigure",
    "LTImage",
    "LTItem",
    "LTLine",
    "LTPage",
    "LTRect",
    "LTTextBox",
    "LTTextBoxHorizontal",
    "LTTextBoxVertical",
    "LTTextContainer",
    "LTTextLine",
    "LTTextLineHorizontal",
    "LTTextLineVertical",
]
