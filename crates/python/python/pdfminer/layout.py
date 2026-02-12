# pdfminer.layout compatibility shim
#
# Layout analysis types backed by bolivar's PyO3 classes.

import abc
from collections.abc import Generator, Iterable, Iterator
from typing import Protocol, TypeGuard

from bolivar._native_api import (
    INF,
    LAParams,
    LTAnno,
    LTChar,
    LTCurve,
    LTFigure,
    LTImage,
    LTLine,
    LTRect,
    Plane,
)
from bolivar._native_api import (
    LTPage as _RustLTPage,
)
from bolivar._native_api import (
    LTTextBoxHorizontal as _RustLTTextBoxHorizontal,
)
from bolivar._native_api import (
    LTTextBoxVertical as _RustLTTextBoxVertical,
)
from bolivar._native_api import (
    LTTextLineHorizontal as _RustLTTextLineHorizontal,
)
from bolivar._native_api import (
    LTTextLineVertical as _RustLTTextLineVertical,
)


class PDFGraphicState:
    """Graphics state stub for pdfplumber compatibility.

    pdfplumber accesses obj.graphicstate.scolor/.ncolor for colors.
    """

    def __init__(self) -> None:
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


class LTItem(metaclass=abc.ABCMeta):  # noqa: B024
    """Marker base class for layout items."""


class _HasBBox(Protocol):
    x0: float
    y0: float
    x1: float
    y1: float
    width: float
    height: float


class _LAParamsLike(Protocol):
    line_margin: float


def _is_bbox_obj(obj: object) -> TypeGuard[_HasBBox]:
    return all(
        hasattr(obj, name) for name in ("x0", "y0", "x1", "y1", "width", "height")
    )


class LTComponent(LTItem):
    """Object with a bounding box."""

    def __init__(self, bbox: tuple[float, float, float, float]) -> None:
        self.set_bbox(bbox)

    def set_bbox(self, bbox: tuple[float, float, float, float]) -> None:
        x0, y0, x1, y1 = bbox
        self.x0 = x0
        self.y0 = y0
        self.x1 = x1
        self.y1 = y1
        self.width = x1 - x0
        self.height = y1 - y0
        self.bbox = (x0, y0, x1, y1)

    def is_empty(self) -> bool:
        return self.width <= 0 or self.height <= 0

    def is_hoverlap(self, obj: _HasBBox) -> bool:
        return obj.x0 <= self.x1 and self.x0 <= obj.x1

    def hdistance(self, obj: _HasBBox) -> float:
        if self.is_hoverlap(obj):
            return 0
        return min(abs(self.x0 - obj.x1), abs(self.x1 - obj.x0))

    def hoverlap(self, obj: _HasBBox) -> float:
        return max(0, min(self.x1, obj.x1) - max(self.x0, obj.x0))

    def is_voverlap(self, obj: _HasBBox) -> bool:
        return obj.y0 <= self.y1 and self.y0 <= obj.y1

    def vdistance(self, obj: _HasBBox) -> float:
        if self.is_voverlap(obj):
            return 0
        return min(abs(self.y0 - obj.y1), abs(self.y1 - obj.y0))

    def voverlap(self, obj: _HasBBox) -> float:
        return max(0, min(self.y1, obj.y1) - max(self.y0, obj.y0))


class LTContainer(LTComponent):
    """Object that can be extended and analyzed."""

    def __init__(self, bbox: tuple[float, float, float, float]) -> None:
        super().__init__(bbox)
        self._objs: list[object] = []

    def __iter__(self) -> Iterator[object]:
        return iter(self._objs)

    def __len__(self) -> int:
        return len(self._objs)

    def add(self, obj: object) -> None:
        self._objs.append(obj)

    def extend(self, objs: Iterable[object]) -> None:
        for obj in objs:
            self.add(obj)


class LTTextContainer(LTContainer):
    """Base class for text containers with expandable bbox."""

    def __init__(self) -> None:
        super().__init__((INF, INF, -INF, -INF))

    def add(self, obj: object) -> None:
        super().add(obj)
        if not _is_bbox_obj(obj):
            return
        self.set_bbox(
            (
                min(self.x0, obj.x0),
                min(self.y0, obj.y0),
                max(self.x1, obj.x1),
                max(self.y1, obj.y1),
            )
        )

    def get_text(self) -> str:
        parts: list[str] = []
        for obj in self:
            getter = getattr(obj, "get_text", None)
            if callable(getter):
                parts.append(str(getter()))
        return "".join(parts)


class LTLayoutContainer(LTContainer):
    """Layout container that can group text lines into text boxes."""

    def __init__(self, bbox: tuple[float, float, float, float]) -> None:
        super().__init__(bbox)
        self.groups = None

    def group_textlines(
        self, laparams: _LAParamsLike, lines: Iterable["LTTextLine"]
    ) -> Generator["LTTextBox", None, None]:
        from .utils import Plane

        plane = Plane(self.bbox)
        lines = list(lines)
        plane.extend(lines)

        boxes: dict[object, LTTextBox] = {}
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


class LTTextLine(LTTextContainer):
    """Base class for text lines."""

    def __init__(self, word_margin: float) -> None:
        super().__init__()
        self.word_margin = word_margin

    def find_neighbors(self, plane: Plane, ratio: float) -> list["LTTextLine"]:
        raise NotImplementedError


class LTTextBox(LTTextContainer):
    """Base class for text boxes."""

    def __init__(self) -> None:
        super().__init__()
        self.index = -1


class LTTextLineHorizontal(LTTextLine):
    def __init__(self, word_margin: float) -> None:
        super().__init__(word_margin)
        self._x1 = INF

    def find_neighbors(self, plane: Plane, ratio: float) -> list["LTTextLine"]:
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

    def _is_left_aligned_with(self, other: _HasBBox, tolerance: float = 0) -> bool:
        return abs(other.x0 - self.x0) <= tolerance

    def _is_right_aligned_with(self, other: _HasBBox, tolerance: float = 0) -> bool:
        return abs(other.x1 - self.x1) <= tolerance

    def _is_centrally_aligned_with(self, other: _HasBBox, tolerance: float = 0) -> bool:
        return abs((other.x0 + other.x1) / 2 - (self.x0 + self.x1) / 2) <= tolerance

    def _is_same_height_as(self, other: _HasBBox, tolerance: float = 0) -> bool:
        return abs(other.height - self.height) <= tolerance


class LTTextLineVertical(LTTextLine):
    def __init__(self, word_margin: float) -> None:
        super().__init__(word_margin)
        self._y0 = -INF

    def find_neighbors(self, plane: Plane, ratio: float) -> list["LTTextLine"]:
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

    def _is_lower_aligned_with(self, other: _HasBBox, tolerance: float = 0) -> bool:
        return abs(other.y0 - self.y0) <= tolerance

    def _is_upper_aligned_with(self, other: _HasBBox, tolerance: float = 0) -> bool:
        return abs(other.y1 - self.y1) <= tolerance

    def _is_centrally_aligned_with(self, other: _HasBBox, tolerance: float = 0) -> bool:
        return abs((other.y0 + other.y1) / 2 - (self.y0 + self.y1) / 2) <= tolerance

    def _is_same_width_as(self, other: _HasBBox, tolerance: float = 0) -> bool:
        return abs(other.width - self.width) <= tolerance


class LTTextBoxHorizontal(LTTextBox):
    pass


class LTTextBoxVertical(LTTextBox):
    pass


def _wrap_rust_item(item: object) -> object:
    return item


_PAGE_SKIP_TYPES = (
    LTTextLineHorizontal,
    LTTextLineVertical,
    _RustLTTextLineHorizontal,
    _RustLTTextLineVertical,
)


def _iter_page_items(item: object) -> Generator[object, None, None]:
    wrapped = _wrap_rust_item(item)
    if not isinstance(wrapped, _PAGE_SKIP_TYPES):
        yield wrapped
    if isinstance(wrapped, LTContainer):
        for child in wrapped:
            yield from _iter_page_items(child)


def _uniq(objs: Iterable[object]) -> Generator[object, None, None]:
    seen = set()
    for obj in objs:
        marker = id(obj)
        if marker in seen:
            continue
        seen.add(marker)
        yield obj


class LTPage(LTLayoutContainer):
    def __init__(self, page: object) -> None:
        if isinstance(page, LTPage):
            page = page._page
        if not isinstance(page, _RustLTPage):
            raise TypeError("LTPage expects a rust LTPage")
        self._page = page
        self.pageid = page.pageid
        self.rotate = page.rotate
        self.bbox = page.bbox
        self.groups = None

    @property
    def x0(self) -> float:
        return self.bbox[0]

    @property
    def y0(self) -> float:
        return self.bbox[1]

    @property
    def x1(self) -> float:
        return self.bbox[2]

    @property
    def y1(self) -> float:
        return self.bbox[3]

    @property
    def width(self) -> float:
        return self.x1 - self.x0

    @property
    def height(self) -> float:
        return self.y1 - self.y0

    @property
    def _objs(self) -> list[object]:
        return [_wrap_rust_item(obj) for obj in self._page]

    def __iter__(self) -> Generator[object, None, None]:
        for item in self._page:
            yield from _iter_page_items(item)

    def __len__(self) -> int:
        return len(self._page)


_ITEM_TYPES = (
    LTAnno,
    LTChar,
    LTCurve,
    LTLine,
    LTRect,
    LTImage,
    LTFigure,
    _RustLTTextLineHorizontal,
    _RustLTTextLineVertical,
    _RustLTTextBoxHorizontal,
    _RustLTTextBoxVertical,
    LTPage,
)

_CONTAINER_TYPES = (
    LTPage,
    LTFigure,
    _RustLTTextLineHorizontal,
    _RustLTTextLineVertical,
    _RustLTTextBoxHorizontal,
    _RustLTTextBoxVertical,
)

_TEXT_LINE_TYPES = (
    _RustLTTextLineHorizontal,
    _RustLTTextLineVertical,
)

_TEXT_BOX_TYPES = (
    _RustLTTextBoxHorizontal,
    _RustLTTextBoxVertical,
)

_TEXT_CONTAINER_TYPES = _TEXT_LINE_TYPES + _TEXT_BOX_TYPES

_COMPONENT_TYPES = tuple(t for t in _ITEM_TYPES if t is not LTAnno)

_RUST_TYPES = (
    LTAnno,
    LTChar,
    LTCurve,
    LTLine,
    LTRect,
    LTImage,
    LTFigure,
    _RustLTTextLineHorizontal,
    _RustLTTextLineVertical,
    _RustLTTextBoxHorizontal,
    _RustLTTextBoxVertical,
)

for _cls in _RUST_TYPES:
    _cls.__module__ = "bolivar._bolivar"

for _cls in _ITEM_TYPES:
    LTItem.register(_cls)
for _cls in _COMPONENT_TYPES:
    LTComponent.register(_cls)
for _cls in _CONTAINER_TYPES:
    LTContainer.register(_cls)
    LTLayoutContainer.register(_cls)
for _cls in _TEXT_CONTAINER_TYPES:
    LTTextContainer.register(_cls)
for _cls in _TEXT_LINE_TYPES:
    LTTextLine.register(_cls)
for _cls in _TEXT_BOX_TYPES:
    LTTextBox.register(_cls)

# Keep isinstance checks working for extracted Rust classes even though
# pdfminer.layout also exposes constructible Python compatibility classes.
LTTextLineHorizontal.register(_RustLTTextLineHorizontal)
LTTextLineVertical.register(_RustLTTextLineVertical)
LTTextBoxHorizontal.register(_RustLTTextBoxHorizontal)
LTTextBoxVertical.register(_RustLTTextBoxVertical)


def _container_objs(self: Iterable[object]) -> list[object]:
    return list(self)


for _cls in _CONTAINER_TYPES:
    if _cls is LTPage:
        continue
    _cls._objs = property(_container_objs)  # type: ignore[invalid-assignment]


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
