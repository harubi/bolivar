# pdfminer.layout compatibility shim
#
# Layout analysis types. Currently stubs - will be replaced with PyO3 bindings.

from bolivar._bolivar import LAParams
from pdfminer.psparser import PSLiteral
from typing import Iterator, List, Tuple

from .utils import INF, Plane


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
        self.width = self.x1 - self.x0
        self.height = self.y1 - self.y0
        self.bbox = bbox

    def __repr__(self):
        return f"<{self.__class__.__name__} {self.bbox}>"


class LTComponent(LTItem):
    """A layout component with set_bbox method."""

    def set_bbox(self, bbox: Tuple[float, float, float, float]):
        self.x0, self.y0, self.x1, self.y1 = bbox
        self.width = self.x1 - self.x0
        self.height = self.y1 - self.y0
        self.bbox = bbox

    def is_empty(self) -> bool:
        return self.width <= 0 or self.height <= 0

    def is_hoverlap(self, obj: "LTComponent") -> bool:
        return obj.x0 <= self.x1 and self.x0 <= obj.x1

    def hdistance(self, obj: "LTComponent") -> float:
        if self.is_hoverlap(obj):
            return 0
        return min(abs(self.x0 - obj.x1), abs(self.x1 - obj.x0))

    def hoverlap(self, obj: "LTComponent") -> float:
        if self.is_hoverlap(obj):
            return min(abs(self.x0 - obj.x1), abs(self.x1 - obj.x0))
        return 0

    def is_voverlap(self, obj: "LTComponent") -> bool:
        return obj.y0 <= self.y1 and self.y0 <= obj.y1

    def vdistance(self, obj: "LTComponent") -> float:
        if self.is_voverlap(obj):
            return 0
        return min(abs(self.y0 - obj.y1), abs(self.y1 - obj.y0))

    def voverlap(self, obj: "LTComponent") -> float:
        if self.is_voverlap(obj):
            return min(abs(self.y0 - obj.y1), abs(self.y1 - obj.y0))
        return 0


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

    def __init__(
        self, bbox: Tuple[float, float, float, float] = (INF, INF, -INF, -INF)
    ):
        super().__init__(bbox)

    def add(self, obj: LTItem):
        super().add(obj)
        if isinstance(obj, LTComponent):
            self.set_bbox(
                (
                    min(self.x0, obj.x0),
                    min(self.y0, obj.y0),
                    max(self.x1, obj.x1),
                    max(self.y1, obj.y1),
                )
            )

    def get_text(self) -> str:
        """Get text content."""
        return "".join(
            obj.get_text() if hasattr(obj, "get_text") else "" for obj in self._objs
        )


class LTAnno(LTItem):
    """Virtual annotation (space, newline) inserted during analysis."""

    def __init__(self, text: str):
        super().__init__((0, 0, 0, 0))
        self._text = text
        # Store as 'text' in __dict__ for pdfplumber compatibility
        self.text = text

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
            self.matrix = _rust_char.matrix
            self.font = None
            self.scaling = 1.0
            self.rise = 0
            self.textwidth = _rust_char.adv
            self.adv = _rust_char.adv  # Alias for pdfplumber
            self.upright = bool(getattr(_rust_char, "upright", True))
            self.textdisp = None
            # Create graphicstate with actual colors from Rust
            gs = PDFGraphicState()

            # pdfplumber expects colors as tuples - (r, g, b) or (gray,)
            def _normalize_gray(vals):
                if (
                    len(vals) == 1
                    and isinstance(vals[0], float)
                    and vals[0].is_integer()
                ):
                    return (int(vals[0]),)
                return tuple(vals)

            if (
                hasattr(_rust_char, "non_stroking_color")
                and _rust_char.non_stroking_color
            ):
                gs.ncolor = _normalize_gray(_rust_char.non_stroking_color)
            if hasattr(_rust_char, "stroking_color") and _rust_char.stroking_color:
                gs.scolor = _normalize_gray(_rust_char.stroking_color)
            self.graphicstate = gs
            try:
                ncs_val = _rust_char.ncs
            except Exception:
                ncs_val = None
            self.ncs = (
                ncs_val
                if isinstance(ncs_val, PSLiteral)
                else PSLiteral(ncs_val or "DeviceGray")
            )
            gs.ncs = self.ncs
            try:
                scs_val = _rust_char.scs
            except Exception:
                scs_val = None
            self.scs = (
                scs_val
                if isinstance(scs_val, PSLiteral)
                else PSLiteral(scs_val or "DeviceGray")
            )
            gs.scs = self.scs
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

    @classmethod
    def from_rust(cls, rust_curve) -> "LTCurve":
        """Create an LTCurve from a Rust LTCurve."""
        # Get colors from Rust
        non_stroking = (
            tuple(rust_curve.non_stroking_color)
            if rust_curve.non_stroking_color
            else None
        )
        stroking = (
            tuple(rust_curve.stroking_color) if rust_curve.stroking_color else None
        )
        # Get points
        pts = list(rust_curve.pts)
        curve = cls(
            linewidth=rust_curve.linewidth,
            pts=pts,
            stroke=rust_curve.stroke,
            fill=rust_curve.fill,
            evenodd=rust_curve.evenodd,
            non_stroking_color=non_stroking,
            stroking_color=stroking,
        )
        if hasattr(rust_curve, "mcid"):
            curve.mcid = rust_curve.mcid
        if hasattr(rust_curve, "tag"):
            curve.tag = rust_curve.tag
        # Copy original_path from Rust (pdfplumber expects this)
        if rust_curve.original_path:
            curve.original_path = [(cmd, *pts) for cmd, pts in rust_curve.original_path]
        else:
            curve.original_path = None
        # Copy dashing_style
        curve.dashing_style = rust_curve.dashing_style
        # Delete ncs/scs if None to avoid pdfplumber AttributeError
        if curve.ncs is None:
            del curve.ncs
        if curve.scs is None:
            del curve.scs
        # Store height/width as instance attrs for pdfplumber __dict__ iteration
        # Use __dict__ to shadow the @property from parent class
        curve.__dict__["height"] = curve.y1 - curve.y0
        curve.__dict__["width"] = curve.x1 - curve.x0
        return curve


class LTLine(LTCurve):
    """A straight line."""

    @classmethod
    def from_rust(cls, rust_line) -> "LTLine":
        """Create an LTLine from a Rust LTLine."""
        # Get colors from Rust
        non_stroking = (
            tuple(rust_line.non_stroking_color)
            if rust_line.non_stroking_color
            else None
        )
        stroking = tuple(rust_line.stroking_color) if rust_line.stroking_color else None
        # Get points
        pts = list(rust_line.pts)
        line = cls(
            linewidth=rust_line.linewidth,
            pts=pts,
            stroke=rust_line.stroke,
            fill=rust_line.fill,
            non_stroking_color=non_stroking,
            stroking_color=stroking,
        )
        if hasattr(rust_line, "mcid"):
            line.mcid = rust_line.mcid
        if hasattr(rust_line, "tag"):
            line.tag = rust_line.tag
        # Copy original_path from Rust (pdfplumber expects this)
        if rust_line.original_path:
            line.original_path = [(cmd, *pts) for cmd, pts in rust_line.original_path]
        else:
            line.original_path = None
        # Copy dashing_style
        line.dashing_style = rust_line.dashing_style
        # Delete ncs/scs if None to avoid pdfplumber AttributeError
        if line.ncs is None:
            del line.ncs
        if line.scs is None:
            del line.scs
        # Store height/width as instance attrs for pdfplumber __dict__ iteration
        # Use __dict__ to shadow the @property from parent class
        line.__dict__["height"] = line.y1 - line.y0
        line.__dict__["width"] = line.x1 - line.x0
        return line


class LTRect(LTCurve):
    """A rectangle."""

    @classmethod
    def from_rust(cls, rust_rect) -> "LTRect":
        """Create an LTRect from a Rust LTRect."""
        bbox = rust_rect.bbox
        # Get colors from Rust
        non_stroking = (
            tuple(rust_rect.non_stroking_color)
            if rust_rect.non_stroking_color
            else None
        )
        stroking = tuple(rust_rect.stroking_color) if rust_rect.stroking_color else None
        # Create with pts from bbox (4 corners of rectangle)
        x0, y0, x1, y1 = bbox
        pts = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
        rect = cls(
            linewidth=rust_rect.linewidth,
            pts=pts,
            stroke=rust_rect.stroke,
            fill=rust_rect.fill,
            non_stroking_color=non_stroking,
            stroking_color=stroking,
        )
        if hasattr(rust_rect, "mcid"):
            rect.mcid = rust_rect.mcid
        if hasattr(rust_rect, "tag"):
            rect.tag = rust_rect.tag
        # Copy original_path from Rust (pdfplumber expects this)
        # Format: [(cmd, pt1, pt2, ...), ...] not [(cmd, [pt1, ...]), ...]
        if rust_rect.original_path:
            rect.original_path = [(cmd, *pts) for cmd, pts in rust_rect.original_path]
        else:
            rect.original_path = None
        # Copy dashing_style from Rust
        rect.dashing_style = rust_rect.dashing_style
        # pdfplumber checks hasattr(obj, 'ncs') and accesses .name
        # Delete these attrs if None to avoid AttributeError
        if rect.ncs is None:
            del rect.ncs
        if rect.scs is None:
            del rect.scs
        # Store height/width as instance attrs for pdfplumber __dict__ iteration
        # Use __dict__ to shadow the @property from parent class
        rect.__dict__["height"] = rect.y1 - rect.y0
        rect.__dict__["width"] = rect.x1 - rect.x0
        return rect


class LTFigure(LTContainer):
    """A figure (Form XObject) in the PDF."""

    def __init__(self, name: str, bbox: Tuple[float, float, float, float], matrix):
        super().__init__(bbox)
        self.name = name
        self.matrix = matrix

    @classmethod
    def from_rust(cls, rust_figure) -> "LTFigure":
        figure = cls(rust_figure.name, rust_figure.bbox, rust_figure.matrix)
        for child in rust_figure:
            wrapped = _wrap_rust_item(child)
            if wrapped is not None:
                figure.add(wrapped)
        return figure


class LTTextLine(LTTextContainer):
    """A line of text."""

    def __init__(self, word_margin: float):
        super().__init__((0, 0, 0, 0))
        self.word_margin = word_margin

    def find_neighbors(self, plane: Plane, ratio: float):
        raise NotImplementedError


class LTTextLineHorizontal(LTTextLine):
    """A horizontal line of text."""

    def __init__(self, word_margin: float):
        super().__init__(word_margin)
        self._x1 = INF

    def add(self, obj: LTComponent):
        if isinstance(obj, LTChar) and self.word_margin:
            margin = self.word_margin * max(obj.width, obj.height)
            if self._x1 < obj.x0 - margin:
                LTContainer.add(self, LTAnno(" "))
        self._x1 = obj.x1
        super().add(obj)

    def find_neighbors(self, plane: Plane, ratio: float):
        d = ratio * self.height
        objs = plane.find((self.x0, self.y0 - d, self.x1, self.y1 + d))
        return [
            obj
            for obj in objs
            if isinstance(obj, LTTextLineHorizontal)
            and self._is_same_height_as(obj, tolerance=d)
            and (
                self._is_left_aligned_with(obj, tolerance=d)
                or self._is_right_aligned_with(obj, tolerance=d)
                or self._is_centrally_aligned_with(obj, tolerance=d)
            )
        ]

    def _is_left_aligned_with(self, other: LTComponent, tolerance: float = 0) -> bool:
        return abs(other.x0 - self.x0) <= tolerance

    def _is_right_aligned_with(self, other: LTComponent, tolerance: float = 0) -> bool:
        return abs(other.x1 - self.x1) <= tolerance

    def _is_centrally_aligned_with(
        self, other: LTComponent, tolerance: float = 0
    ) -> bool:
        return abs((other.x0 + other.x1) / 2 - (self.x0 + self.x1) / 2) <= tolerance

    def _is_same_height_as(self, other: LTComponent, tolerance: float = 0) -> bool:
        return abs(other.height - self.height) <= tolerance

    @classmethod
    def from_rust(cls, rust_line) -> "LTTextLineHorizontal":
        line = cls(0)
        line.set_bbox(rust_line.bbox)
        for child in rust_line:
            wrapped = _wrap_rust_item(child)
            if wrapped is not None:
                line.add(wrapped)
        return line


class LTTextLineVertical(LTTextLine):
    """A vertical line of text."""

    def __init__(self, word_margin: float):
        super().__init__(word_margin)
        self._y0 = -INF

    def add(self, obj: LTComponent):
        if isinstance(obj, LTChar) and self.word_margin:
            margin = self.word_margin * max(obj.width, obj.height)
            if obj.y1 + margin < self._y0:
                LTContainer.add(self, LTAnno(" "))
        self._y0 = obj.y0
        super().add(obj)

    def find_neighbors(self, plane: Plane, ratio: float):
        d = ratio * self.width
        objs = plane.find((self.x0 - d, self.y0, self.x1 + d, self.y1))
        return [
            obj
            for obj in objs
            if isinstance(obj, LTTextLineVertical)
            and self._is_same_width_as(obj, tolerance=d)
            and (
                self._is_lower_aligned_with(obj, tolerance=d)
                or self._is_upper_aligned_with(obj, tolerance=d)
                or self._is_centrally_aligned_with(obj, tolerance=d)
            )
        ]

    def _is_lower_aligned_with(self, other: LTComponent, tolerance: float = 0) -> bool:
        return abs(other.y0 - self.y0) <= tolerance

    def _is_upper_aligned_with(self, other: LTComponent, tolerance: float = 0) -> bool:
        return abs(other.y1 - self.y1) <= tolerance

    def _is_centrally_aligned_with(
        self, other: LTComponent, tolerance: float = 0
    ) -> bool:
        return abs((other.y0 + other.y1) / 2 - (self.y0 + self.y1) / 2) <= tolerance

    def _is_same_width_as(self, other: LTComponent, tolerance: float) -> bool:
        return abs(other.width - self.width) <= tolerance

    @classmethod
    def from_rust(cls, rust_line) -> "LTTextLineVertical":
        line = cls(0)
        line.set_bbox(rust_line.bbox)
        for child in rust_line:
            wrapped = _wrap_rust_item(child)
            if wrapped is not None:
                line.add(wrapped)
        return line


class LTTextBox(LTTextContainer):
    """A box containing multiple lines of text."""

    index: int = 0

    def __init__(self, bbox: Tuple[float, float, float, float] = (0, 0, 0, 0)):
        super().__init__(bbox)
        self.index = 0


class LTTextBoxHorizontal(LTTextBox):
    """A horizontal text box."""

    @classmethod
    def from_rust(cls, rust_box) -> "LTTextBoxHorizontal":
        box = cls(rust_box.bbox)
        for child in rust_box:
            wrapped = _wrap_rust_item(child)
            if wrapped is not None:
                box.add(wrapped)
        return box


class LTTextBoxVertical(LTTextBox):
    """A vertical text box."""

    @classmethod
    def from_rust(cls, rust_box) -> "LTTextBoxVertical":
        box = cls(rust_box.bbox)
        for child in rust_box:
            wrapped = _wrap_rust_item(child)
            if wrapped is not None:
                box.add(wrapped)
        return box


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

    @classmethod
    def from_rust(cls, rust_image) -> "LTImage":
        image = cls(rust_image.name, None, rust_image.bbox)
        image.srcsize = rust_image.srcsize
        image.imagemask = rust_image.imagemask
        image.bits = rust_image.bits
        image.colorspace = rust_image.colorspace
        return image


def _uniq(items):
    seen = set()
    out = []
    for item in items:
        if item in seen:
            continue
        seen.add(item)
        out.append(item)
    return out


class LTLayoutContainer(LTContainer):
    """Container that performs layout grouping."""

    def __init__(self, bbox: Tuple[float, float, float, float]):
        super().__init__(bbox)
        self.groups = None

    def group_textlines(self, laparams: LAParams, lines):
        plane = Plane(self.bbox)
        plane.extend(lines)
        boxes = {}
        for line in lines:
            neighbors = line.find_neighbors(plane, laparams.line_margin)
            members = [line]
            for obj1 in neighbors:
                members.append(obj1)
                if obj1 in boxes:
                    members.extend(boxes.pop(obj1))
            if isinstance(line, LTTextLineHorizontal):
                box = LTTextBoxHorizontal()
            else:
                box = LTTextBoxVertical()
            for obj in _uniq(members):
                box.add(obj)
                boxes[obj] = box
        done = set()
        for line in lines:
            if line not in boxes:
                continue
            box = boxes[line]
            if box in done:
                continue
            done.add(box)
            if not box.is_empty():
                yield box


def _wrap_rust_item(rust_item):
    type_name = type(rust_item).__name__
    if type_name == "LTChar":
        return LTChar.from_rust(rust_item)
    if type_name == "LTAnno":
        return LTAnno(rust_item.get_text())
    if type_name == "LTRect":
        return LTRect.from_rust(rust_item)
    if type_name == "LTLine":
        return LTLine.from_rust(rust_item)
    if type_name == "LTCurve":
        return LTCurve.from_rust(rust_item)
    if type_name == "LTTextLineHorizontal":
        return LTTextLineHorizontal.from_rust(rust_item)
    if type_name == "LTTextLineVertical":
        return LTTextLineVertical.from_rust(rust_item)
    if type_name == "LTTextBoxHorizontal":
        return LTTextBoxHorizontal.from_rust(rust_item)
    if type_name == "LTTextBoxVertical":
        return LTTextBoxVertical.from_rust(rust_item)
    if type_name == "LTImage":
        return LTImage.from_rust(rust_item)
    if type_name == "LTFigure":
        return LTFigure.from_rust(rust_item)
    if type_name == "LTPage":
        return LTPage(rust_item)
    return None


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
        if hasattr(pageid_or_rust, "pageid") and hasattr(pageid_or_rust, "bbox"):
            # Wrapping a Rust LTPage
            rust_page = pageid_or_rust
            self._rust_page = rust_page
            super().__init__(rust_page.bbox)
            self.pageid = rust_page.pageid
            self.rotate = int(rust_page.rotate)
            self._objs_ready = False
            self._len = None
        else:
            # Traditional pdfminer.six constructor
            self._rust_page = None
            pageid = pageid_or_rust if pageid_or_rust is not None else 0
            super().__init__(bbox)
            self.pageid = pageid
            self.rotate = rotate
            self._objs_ready = True
            self._len = len(self._objs)

    def __iter__(self) -> Iterator[LTItem]:
        self._ensure_objs()
        return iter(self._objs)

    def _iter_layout(self, objs: List[LTItem]) -> Iterator[LTItem]:
        for obj in objs:
            yield obj
            if isinstance(obj, LTContainer):
                yield from self._iter_layout(obj._objs)

    def __len__(self) -> int:
        if getattr(self, "_rust_page", None) is not None and not getattr(
            self, "_objs_ready", True
        ):
            cached = getattr(self, "_len", None)
            if cached is None:
                cached = len(self._rust_page)
                self._len = cached
            return cached
        return len(object.__getattribute__(self, "_objs"))

    def __getattribute__(self, name: str):
        if name == "_objs":
            object.__getattribute__(self, "_ensure_objs")()
        return object.__getattribute__(self, name)

    def _ensure_objs(self) -> None:
        rust_page = object.__getattribute__(self, "_rust_page")
        if rust_page is None:
            return
        if object.__getattribute__(self, "_objs_ready"):
            return
        object.__setattr__(self, "_objs_ready", True)
        objs = object.__getattribute__(self, "_objs")
        for rust_item in rust_page:
            wrapped = _wrap_rust_item(rust_item)
            if wrapped is not None:
                objs.append(wrapped)


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
    "LTLayoutContainer",
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
