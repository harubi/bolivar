"""Single runtime boundary for native bolivar symbols.

This module exposes native extension attributes lazily so importing one symbol
does not require eagerly resolving every symbol from the extension.
"""

from importlib import import_module
from types import ModuleType
from typing import Any

_NATIVE_MODULE: ModuleType | None = None


def load_native_api() -> ModuleType:
    """Load and memoize the native extension module."""
    global _NATIVE_MODULE
    if _NATIVE_MODULE is None:
        _NATIVE_MODULE = import_module("bolivar._bolivar")
    return _NATIVE_MODULE


Arcfour: Any
CCITTFaxDecoder: Any
CCITTG4Parser: Any
CMap: Any
CMapDB: Any
EncodingDB: Any
HOCRConverter: Any
HTMLConverter: Any
INF: Any
IdentityCMap: Any
IdentityCMapByte: Any
ImageWriter: Any
KWD: Any
LAParams: Any
LIT: Any
LTAnno: Any
LTChar: Any
LTCurve: Any
LTFigure: Any
LTImage: Any
LTLine: Any
LTPage: Any
LTRect: Any
LTTextBoxHorizontal: Any
LTTextBoxVertical: Any
LTTextLineHorizontal: Any
LTTextLineVertical: Any
MATRIX_IDENTITY: Any
NumberTree: Any
PDFCIDFont: Any
PDFDocEncoding: Any
PDFDocument: Any
PDFFont: Any
PDFPage: Any
PDFParser: Any
PDFResourceManager: Any
PDFStream: Any
PSBaseParser: Any
PSKeyword: Any
PSLiteral: Any
PSStackParser: Any
Plane: Any
TagExtractor: Any
TextConverter: Any
UnicodeMap: Any
XMLConverter: Any
__version__: Any
_extract_tables_core: Any
_extract_tables_from_page_objects: Any
apply_matrix_pt: Any
apply_matrix_rect: Any
ascii85decode: Any
asciihexdecode: Any
async_runtime_poc: Any
decode_text: Any
extract_pages: Any
extract_pages_async: Any
extract_pages_from_path: Any
extract_pages_with_images: Any
extract_pages_with_images_from_path: Any
extract_tables_from_document: Any
extract_tables_stream_from_document: Any
extract_text: Any
extract_text_from_path: Any
font_metrics: Any
format_int_alpha: Any
format_int_roman: Any
get_widths: Any
glyphname2unicode: Any
isnumber: Any
latin_encoding: Any
lzwdecode: Any
lzwdecode_with_earlychange: Any
mult_matrix: Any
name2unicode: Any
process_page: Any
process_pages: Any
reorder_text_for_output: Any
repair_pdf: Any
rldecode: Any
safe_cmyk: Any
safe_float: Any
safe_int: Any
safe_matrix: Any
safe_rect: Any
safe_rect_list: Any
safe_rgb: Any
shorten_str: Any
translate_matrix: Any
unpad_aes: Any

__all__ = [
    "INF",
    "KWD",
    "LIT",
    "MATRIX_IDENTITY",
    "Arcfour",
    "CCITTFaxDecoder",
    "CCITTG4Parser",
    "CMap",
    "CMapDB",
    "EncodingDB",
    "HOCRConverter",
    "HTMLConverter",
    "IdentityCMap",
    "IdentityCMapByte",
    "ImageWriter",
    "LAParams",
    "LTAnno",
    "LTChar",
    "LTCurve",
    "LTFigure",
    "LTImage",
    "LTLine",
    "LTPage",
    "LTRect",
    "LTTextBoxHorizontal",
    "LTTextBoxVertical",
    "LTTextLineHorizontal",
    "LTTextLineVertical",
    "NumberTree",
    "PDFCIDFont",
    "PDFDocEncoding",
    "PDFDocument",
    "PDFFont",
    "PDFPage",
    "PDFParser",
    "PDFResourceManager",
    "PDFStream",
    "PSBaseParser",
    "PSKeyword",
    "PSLiteral",
    "PSStackParser",
    "Plane",
    "TagExtractor",
    "TextConverter",
    "UnicodeMap",
    "XMLConverter",
    "__version__",
    "_extract_tables_core",
    "_extract_tables_from_page_objects",
    "apply_matrix_pt",
    "apply_matrix_rect",
    "ascii85decode",
    "asciihexdecode",
    "async_runtime_poc",
    "decode_text",
    "extract_pages",
    "extract_pages_async",
    "extract_pages_from_path",
    "extract_pages_with_images",
    "extract_pages_with_images_from_path",
    "extract_tables_from_document",
    "extract_tables_stream_from_document",
    "extract_text",
    "extract_text_from_path",
    "font_metrics",
    "format_int_alpha",
    "format_int_roman",
    "get_widths",
    "glyphname2unicode",
    "isnumber",
    "latin_encoding",
    "lzwdecode",
    "lzwdecode_with_earlychange",
    "mult_matrix",
    "name2unicode",
    "process_page",
    "process_pages",
    "reorder_text_for_output",
    "repair_pdf",
    "rldecode",
    "safe_cmyk",
    "safe_float",
    "safe_int",
    "safe_matrix",
    "safe_rect",
    "safe_rect_list",
    "safe_rgb",
    "shorten_str",
    "translate_matrix",
    "unpad_aes",
]


def __getattr__(name: str) -> object:
    if name not in __all__:
        raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
    native = load_native_api()
    try:
        return getattr(native, name)
    except AttributeError as exc:
        raise AttributeError(
            f"native module bolivar._bolivar has no attribute {name!r}"
        ) from exc
