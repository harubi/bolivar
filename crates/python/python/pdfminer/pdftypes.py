# pdfminer.pdftypes compatibility shim

from collections.abc import Mapping
from typing import Protocol

from bolivar._native_api import PDFStream


class _PDFDocumentLike(Protocol):
    def getobj(self, objid: int) -> object:
        """Resolve a PDF object by id."""


class PDFObjRef:
    """PDF object reference.

    Represents an indirect object reference like "1 0 R".
    """

    def __init__(self, doc: _PDFDocumentLike, objid: int, genno: int = 0) -> None:
        self.doc = doc
        self.objid = objid
        self.genno = genno

    def __repr__(self) -> str:
        return f"<PDFObjRef:{self.objid}>"

    def resolve(self, default: object | None = None) -> object | None:
        """Resolve this reference to its actual object."""
        try:
            return self.doc.getobj(self.objid)
        except Exception:
            return default


def resolve1(x: object, default: object | None = None) -> object | None:
    """Resolve a PDF object reference.

    If x is a PDFObjRef, resolve it. Otherwise return x.
    """
    while isinstance(x, PDFObjRef):
        x = x.resolve(default)
    return x


def resolve_all(x: object, default: object | None = None) -> object:
    """Recursively resolve all PDFObjRef in a structure."""
    if isinstance(x, PDFObjRef):
        return resolve_all(x.resolve(default), default)
    elif isinstance(x, list):
        return [resolve_all(item, default) for item in x]
    elif isinstance(x, Mapping):
        return {k: resolve_all(v, default) for k, v in x.items()}
    return x


def stream_value(x: object) -> object:
    """Get a stream's data."""
    if isinstance(x, PDFStream):
        return x.get_data()
    return x


def int_value(x: object) -> int:
    """Convert to int."""
    if isinstance(x, PDFObjRef):
        x = x.resolve()
    if isinstance(x, int):
        return x
    if isinstance(x, float):
        return int(x)
    if isinstance(x, (str, bytes, bytearray)):
        try:
            return int(x)
        except (TypeError, ValueError):
            return 0
    return 0


def float_value(x: object) -> float:
    """Convert to float."""
    if isinstance(x, PDFObjRef):
        x = x.resolve()
    if isinstance(x, (int, float)):
        return float(x)
    if isinstance(x, (str, bytes, bytearray)):
        try:
            return float(x)
        except (TypeError, ValueError):
            return 0.0
    return 0.0


def str_value(x: object) -> str:
    """Convert to string."""
    if isinstance(x, PDFObjRef):
        x = x.resolve()
    if isinstance(x, bytes):
        return x.decode("latin-1")
    return str(x) if x is not None else ""


def list_value(x: object) -> list[object]:
    """Convert to list."""
    if isinstance(x, PDFObjRef):
        x = x.resolve()
    if isinstance(x, (list, tuple)):
        return list(x)
    return []


def dict_value(x: object) -> dict[object, object]:
    """Convert to dict."""
    if isinstance(x, PDFObjRef):
        x = x.resolve()
    if isinstance(x, Mapping):
        return dict(x.items())
    return {}
