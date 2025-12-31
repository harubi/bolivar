# pdfminer.layout compatibility shim
#
# Layout analysis types. Currently stubs - will be replaced with PyO3 bindings.

from bolivar._bolivar import LAParams
from typing import Iterator, List, Optional, Tuple, Any


class PDFGraphicState:
    """Graphics state stub for pdfplumber compatibility.

    pdfplumber accesses obj.graphicstate.scolor/.ncolor for colors.
    """

    def __init__(self):
        self.linewidth = 0
        self.linecap = None
        self.linejoin = None
        self.miterlimit = None
        self.dash = None
        self.intent = None
        self.flatness = None
        # Default colors (grayscale black)
        self.scolor = 0  # stroking color
        self.ncolor = 0  # non-stroking color
        self.scs = None  # stroking colorspace
        self.ncs = None  # non-stroking colorspace


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
    """A single character with position and font info.

    Can be created from a Rust LTChar or with traditional pdfminer.six args.
    """

    def __init__(
        self,
        matrix=None,
        font=None,
        fontsize: float = 0,
        scaling: float = 1.0,
        rise: float = 0,
        text: str = "",
        textwidth: float = 0,
        textdisp=None,
        ncs=None,
        graphicstate=None,
        *,
        _rust_char=None,
    ):
        if _rust_char is not None:
            # Wrapping a Rust LTChar
            self._rust_char = _rust_char
            bbox = _rust_char.bbox
            super().__init__(bbox)
            self._text = _rust_char.get_text()
            # Store fontname as direct attribute (not property) for pdfplumber
            # pdfplumber iterates obj.__dict__.items() to find attributes
            self.fontname = _rust_char.fontname
            self.fontsize = _rust_char.size
            self.size = _rust_char.size  # Alias for pdfplumber
            self.mcid = _rust_char.mcid
            self.tag = _rust_char.tag
            self.matrix = None
            self.font = None
            self.scaling = 1.0
            self.rise = 0
            self.textwidth = _rust_char.adv
            self.adv = _rust_char.adv  # Alias for pdfplumber
            self.upright = True  # Default - TODO: get from Rust
            self.textdisp = None
            # Provide a stub graphicstate for pdfplumber color access
            self.graphicstate = PDFGraphicState()
            # Don't set ncs - pdfplumber uses hasattr() check for colorspace
            # Setting to None would make hasattr return True but .name access fails
        else:
            # Traditional pdfminer.six constructor
            self._rust_char = None
            self.matrix = matrix
            self.font = font
            self.fontsize = fontsize
            self.size = fontsize
            self.scaling = scaling
            self.rise = rise
            self._text = text
            self.textwidth = textwidth
            self.adv = textwidth  # Alias
            self.upright = True
            self.textdisp = textdisp
            self.ncs = ncs
            self.graphicstate = graphicstate
            # Store fontname directly in __dict__ for pdfplumber
            if font and hasattr(font, "fontname"):
                self.fontname = font.fontname
            else:
                self.fontname = str(font) if font else ""
            self.mcid = None
            self.tag = None

            # Simplified bbox calculation
            if matrix:
                x0 = matrix[4]
                y0 = matrix[5]
                x1 = x0 + textwidth * fontsize
                y1 = y0 + fontsize
            else:
                x0, y0, x1, y1 = 0, 0, 0, 0
            super().__init__((x0, y0, x1, y1))

    @classmethod
    def from_rust(cls, rust_char) -> "LTChar":
        """Create an LTChar from a Rust LTChar."""
        return cls(_rust_char=rust_char)

    def get_text(self) -> str:
        return self._text

    # Note: fontname and size are now direct instance attributes (set in __init__)
    # rather than properties, for pdfplumber compatibility which iterates __dict__


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
    """A page containing layout items.

    Can wrap a Rust LTPage or be created with traditional pdfminer.six args.
    """

    def __init__(
        self,
        pageid_or_rust=None,
        bbox: Tuple[float, float, float, float] = (0, 0, 0, 0),
        rotate: int = 0,
    ):
        # Check if first arg is a Rust LTPage (has pageid attribute and bbox property)
        if hasattr(pageid_or_rust, 'pageid') and hasattr(pageid_or_rust, 'bbox'):
            # Wrapping a Rust LTPage
            rust_page = pageid_or_rust
            self._rust_page = rust_page
            super().__init__(rust_page.bbox)
            self.pageid = rust_page.pageid
            self.rotate = int(rust_page.rotate)

            # Wrap Rust items in Python shims
            for rust_item in rust_page:
                # Currently all items from Rust are LTChar
                self._objs.append(LTChar.from_rust(rust_item))
        else:
            # Traditional pdfminer.six constructor
            self._rust_page = None
            pageid = pageid_or_rust if pageid_or_rust is not None else 0
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
