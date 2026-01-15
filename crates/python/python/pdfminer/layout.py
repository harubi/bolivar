# pdfminer.layout compatibility shim
#
# Layout analysis types backed by bolivar's PyO3 classes.

import abc

from bolivar._bolivar import (
    LAParams,
    LTAnno,
    LTChar,
    LTCurve,
    LTFigure,
    LTImage,
    LTLine,
    LTPage as _RustLTPage,
    LTRect,
    LTTextBoxHorizontal,
    LTTextBoxVertical,
    LTTextLineHorizontal,
    LTTextLineVertical,
)


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


class LTItem(metaclass=abc.ABCMeta):
    """Marker base class for layout items."""


class LTComponent(LTItem):
    """Marker base class for layout components."""


class LTContainer(LTComponent):
    """Marker base class for layout containers."""


class LTTextContainer(LTContainer):
    """Marker base class for text containers."""


class LTLayoutContainer(LTContainer):
    """Marker base class for layout containers that group text."""


class LTTextLine(LTTextContainer):
    """Marker base class for text lines."""


class LTTextBox(LTTextContainer):
    """Marker base class for text boxes."""


def _wrap_rust_item(item):
    return item


def _iter_flatten(item):
    yield _wrap_rust_item(item)
    if isinstance(item, LTContainer):
        for child in item:
            yield from _iter_flatten(child)


class LTPage(LTLayoutContainer):
    def __init__(self, page):
        if isinstance(page, LTPage):
            page = page._page
        if not isinstance(page, _RustLTPage):
            raise TypeError("LTPage expects a rust LTPage")
        self._page = page
        self.pageid = page.pageid
        self.rotate = page.rotate
        self.bbox = page.bbox

    @property
    def x0(self):
        return self.bbox[0]

    @property
    def y0(self):
        return self.bbox[1]

    @property
    def x1(self):
        return self.bbox[2]

    @property
    def y1(self):
        return self.bbox[3]

    @property
    def width(self):
        return self.x1 - self.x0

    @property
    def height(self):
        return self.y1 - self.y0

    @property
    def _objs(self):
        return [_wrap_rust_item(obj) for obj in self._page]

    def __iter__(self):
        for item in self._page:
            yield from _iter_flatten(item)

    def __len__(self):
        return len(self._page)


_ITEM_TYPES = (
    LTAnno,
    LTChar,
    LTCurve,
    LTLine,
    LTRect,
    LTImage,
    LTFigure,
    LTTextLineHorizontal,
    LTTextLineVertical,
    LTTextBoxHorizontal,
    LTTextBoxVertical,
    LTPage,
)

_CONTAINER_TYPES = (
    LTPage,
    LTFigure,
    LTTextLineHorizontal,
    LTTextLineVertical,
    LTTextBoxHorizontal,
    LTTextBoxVertical,
)

_TEXT_LINE_TYPES = (
    LTTextLineHorizontal,
    LTTextLineVertical,
)

_TEXT_BOX_TYPES = (
    LTTextBoxHorizontal,
    LTTextBoxVertical,
)

_TEXT_CONTAINER_TYPES = _TEXT_LINE_TYPES + _TEXT_BOX_TYPES

_COMPONENT_TYPES = tuple(t for t in _ITEM_TYPES if t is not LTAnno)

for _cls in _ITEM_TYPES:
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


def _container_objs(self):
    return list(self)


for _cls in _CONTAINER_TYPES:
    if _cls is LTPage:
        continue
    setattr(_cls, "_objs", property(_container_objs))


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
