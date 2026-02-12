from typing import cast

from pdfminer.pdftypes import PDFObjRef
from pdfminer.psparser import PSLiteral
from pdfminer.utils import PDFDocEncoding

from .exceptions import MalformedPDFException


def decode_text(s: bytes | str) -> str:
    """
    Decodes a PDFDocEncoding string to Unicode.
    Adds py3 compatibility to pdfminer's version.
    """
    if isinstance(s, bytes) and s.startswith(b"\xfe\xff"):
        return str(s[2:], "utf-16be", "ignore")
    try:
        ords = (ord(c) if isinstance(c, str) else c for c in s)
        return "".join(PDFDocEncoding[o] for o in ords)
    except IndexError:
        return str(s)


def resolve_and_decode(obj: object) -> object:
    """Recursively resolve the metadata values."""
    resolver = getattr(obj, "resolve", None)
    if callable(resolver):
        obj = resolver()
    if isinstance(obj, list):
        return list(map(resolve_and_decode, obj))
    elif isinstance(obj, PSLiteral):
        return decode_text(obj.name)
    elif isinstance(obj, (str, bytes)):
        return decode_text(obj)
    elif isinstance(obj, dict):
        return {k: resolve_and_decode(v) for k, v in obj.items()}

    return obj


def decode_psl_list(_list: list[PSLiteral | str]) -> list[str]:
    return [
        decode_text(value.name) if isinstance(value, PSLiteral) else value
        for value in _list
    ]


def resolve(x: object) -> object:
    if isinstance(x, PDFObjRef):
        return x.resolve()
    else:
        return x


def get_dict_type(d: object) -> str | None:
    if not isinstance(d, dict):
        return None
    typed_dict = cast("dict[str, object]", d)
    t = typed_dict.get("Type")
    if isinstance(t, PSLiteral):
        return decode_text(t.name)
    else:
        return cast("str | None", t)


def resolve_all(x: object) -> object:
    """
    Recursively resolves the given object and all the internals.
    """
    if isinstance(x, PDFObjRef):
        resolved = x.resolve()

        # Avoid infinite recursion
        if get_dict_type(resolved) == "Page":
            return x

        try:
            return resolve_all(resolved)
        except RecursionError as e:
            raise MalformedPDFException(e) from e
    elif isinstance(x, (list, tuple)):
        return type(x)(resolve_all(v) for v in x)
    elif isinstance(x, dict):
        exceptions = ["Parent"] if get_dict_type(x) == "Annot" else []
        return {k: v if k in exceptions else resolve_all(v) for k, v in x.items()}
    else:
        return x
