# pdfminer.pdftypes compatibility shim

from typing import Any, Dict, List

from bolivar._bolivar import PDFStream


class PDFObjRef:
    """PDF object reference.

    Represents an indirect object reference like "1 0 R".
    """

    def __init__(self, doc, objid, genno=0):
        self.doc = doc
        self.objid = objid
        self.genno = genno

    def __repr__(self):
        return f"<PDFObjRef:{self.objid}>"

    def resolve(self, default=None):
        """Resolve this reference to its actual object."""
        try:
            return self.doc.getobj(self.objid)
        except Exception:
            return default


def resolve1(x, default=None):
    """Resolve a PDF object reference.

    If x is a PDFObjRef, resolve it. Otherwise return x.
    """
    while isinstance(x, PDFObjRef):
        x = x.resolve(default)
    return x


def resolve_all(x, default=None):
    """Recursively resolve all PDFObjRef in a structure."""
    if isinstance(x, PDFObjRef):
        return resolve_all(x.resolve(default), default)
    elif isinstance(x, list):
        return [resolve_all(item, default) for item in x]
    elif isinstance(x, dict):
        return {k: resolve_all(v, default) for k, v in x.items()}
    return x


def stream_value(x):
    """Get a stream's data."""
    if isinstance(x, PDFStream):
        return x.get_data()
    return x


def int_value(x):
    """Convert to int."""
    if isinstance(x, PDFObjRef):
        x = x.resolve()
    return int(x) if x is not None else 0


def float_value(x):
    """Convert to float."""
    if isinstance(x, PDFObjRef):
        x = x.resolve()
    return float(x) if x is not None else 0.0


def str_value(x):
    """Convert to string."""
    if isinstance(x, PDFObjRef):
        x = x.resolve()
    if isinstance(x, bytes):
        return x.decode("latin-1")
    return str(x) if x is not None else ""


def list_value(x):
    """Convert to list."""
    if isinstance(x, PDFObjRef):
        x = x.resolve()
    if isinstance(x, (list, tuple)):
        return list(x)
    return []


def dict_value(x):
    """Convert to dict."""
    if isinstance(x, PDFObjRef):
        x = x.resolve()
    if isinstance(x, dict):
        return x
    return {}
