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
    LTPage,
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
