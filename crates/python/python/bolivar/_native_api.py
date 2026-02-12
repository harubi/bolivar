"""Single runtime boundary for native bolivar symbols.

This module exposes native extension attributes lazily so importing one symbol
does not require eagerly resolving every symbol from the extension.
"""

from __future__ import annotations

from importlib import import_module
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from types import ModuleType

    from bolivar._bolivar import (
        INF as INF,
    )
    from bolivar._bolivar import (
        KWD as KWD,
    )
    from bolivar._bolivar import (
        LIT as LIT,
    )
    from bolivar._bolivar import (
        MATRIX_IDENTITY as MATRIX_IDENTITY,
    )
    from bolivar._bolivar import (
        Arcfour as Arcfour,
    )
    from bolivar._bolivar import (
        CCITTFaxDecoder as CCITTFaxDecoder,
    )
    from bolivar._bolivar import (
        CCITTG4Parser as CCITTG4Parser,
    )
    from bolivar._bolivar import (
        CMap as CMap,
    )
    from bolivar._bolivar import (
        CMapDB as CMapDB,
    )
    from bolivar._bolivar import (
        EncodingDB as EncodingDB,
    )
    from bolivar._bolivar import (
        HOCRConverter as HOCRConverter,
    )
    from bolivar._bolivar import (
        HTMLConverter as HTMLConverter,
    )
    from bolivar._bolivar import (
        IdentityCMap as IdentityCMap,
    )
    from bolivar._bolivar import (
        IdentityCMapByte as IdentityCMapByte,
    )
    from bolivar._bolivar import (
        ImageWriter as ImageWriter,
    )
    from bolivar._bolivar import (
        LAParams as LAParams,
    )
    from bolivar._bolivar import (
        LTAnno as LTAnno,
    )
    from bolivar._bolivar import (
        LTChar as LTChar,
    )
    from bolivar._bolivar import (
        LTCurve as LTCurve,
    )
    from bolivar._bolivar import (
        LTFigure as LTFigure,
    )
    from bolivar._bolivar import (
        LTImage as LTImage,
    )
    from bolivar._bolivar import (
        LTLine as LTLine,
    )
    from bolivar._bolivar import (
        LTPage as LTPage,
    )
    from bolivar._bolivar import (
        LTRect as LTRect,
    )
    from bolivar._bolivar import (
        LTTextBoxHorizontal as LTTextBoxHorizontal,
    )
    from bolivar._bolivar import (
        LTTextBoxVertical as LTTextBoxVertical,
    )
    from bolivar._bolivar import (
        LTTextLineHorizontal as LTTextLineHorizontal,
    )
    from bolivar._bolivar import (
        LTTextLineVertical as LTTextLineVertical,
    )
    from bolivar._bolivar import (
        NumberTree as NumberTree,
    )
    from bolivar._bolivar import (
        PDFCIDFont as PDFCIDFont,
    )
    from bolivar._bolivar import (
        PDFDocEncoding as PDFDocEncoding,
    )
    from bolivar._bolivar import (
        PDFDocument as PDFDocument,
    )
    from bolivar._bolivar import (
        PDFFont as PDFFont,
    )
    from bolivar._bolivar import (
        PDFPage as PDFPage,
    )
    from bolivar._bolivar import (
        PDFParser as PDFParser,
    )
    from bolivar._bolivar import (
        PDFResourceManager as PDFResourceManager,
    )
    from bolivar._bolivar import (
        PDFStream as PDFStream,
    )
    from bolivar._bolivar import (
        Plane as Plane,
    )
    from bolivar._bolivar import (
        PSBaseParser as PSBaseParser,
    )
    from bolivar._bolivar import (
        PSKeyword as PSKeyword,
    )
    from bolivar._bolivar import (
        PSLiteral as PSLiteral,
    )
    from bolivar._bolivar import (
        PSStackParser as PSStackParser,
    )
    from bolivar._bolivar import (
        PyTableStream as PyTableStream,
    )
    from bolivar._bolivar import (
        TagExtractor as TagExtractor,
    )
    from bolivar._bolivar import (
        TextConverter as TextConverter,
    )
    from bolivar._bolivar import (
        UnicodeMap as UnicodeMap,
    )
    from bolivar._bolivar import (
        XMLConverter as XMLConverter,
    )
    from bolivar._bolivar import (
        __version__ as __version__,
    )
    from bolivar._bolivar import (
        _extract_tables_core as _extract_tables_core,
    )
    from bolivar._bolivar import (
        _extract_tables_from_page_objects as _extract_tables_from_page_objects,
    )
    from bolivar._bolivar import (
        apply_matrix_pt as apply_matrix_pt,
    )
    from bolivar._bolivar import (
        apply_matrix_rect as apply_matrix_rect,
    )
    from bolivar._bolivar import (
        ascii85decode as ascii85decode,
    )
    from bolivar._bolivar import (
        asciihexdecode as asciihexdecode,
    )
    from bolivar._bolivar import (
        async_runtime_poc as async_runtime_poc,
    )
    from bolivar._bolivar import (
        decode_text as decode_text,
    )
    from bolivar._bolivar import (
        extract_pages as extract_pages,
    )
    from bolivar._bolivar import (
        extract_pages_async as extract_pages_async,
    )
    from bolivar._bolivar import (
        extract_pages_async_from_document as extract_pages_async_from_document,
    )
    from bolivar._bolivar import (
        extract_pages_from_path as extract_pages_from_path,
    )
    from bolivar._bolivar import (
        extract_pages_with_images as extract_pages_with_images,
    )
    from bolivar._bolivar import (
        extract_pages_with_images_from_path as extract_pages_with_images_from_path,
    )
    from bolivar._bolivar import (
        extract_tables_from_document as extract_tables_from_document,
    )
    from bolivar._bolivar import (
        extract_tables_stream_from_document as extract_tables_stream_from_document,
    )
    from bolivar._bolivar import (
        extract_text as extract_text,
    )
    from bolivar._bolivar import (
        extract_text_from_path as extract_text_from_path,
    )
    from bolivar._bolivar import (
        font_metrics as font_metrics,
    )
    from bolivar._bolivar import (
        format_int_alpha as format_int_alpha,
    )
    from bolivar._bolivar import (
        format_int_roman as format_int_roman,
    )
    from bolivar._bolivar import (
        get_widths as get_widths,
    )
    from bolivar._bolivar import (
        glyphname2unicode as glyphname2unicode,
    )
    from bolivar._bolivar import (
        isnumber as isnumber,
    )
    from bolivar._bolivar import (
        latin_encoding as latin_encoding,
    )
    from bolivar._bolivar import (
        lzwdecode as lzwdecode,
    )
    from bolivar._bolivar import (
        lzwdecode_with_earlychange as lzwdecode_with_earlychange,
    )
    from bolivar._bolivar import (
        mult_matrix as mult_matrix,
    )
    from bolivar._bolivar import (
        name2unicode as name2unicode,
    )
    from bolivar._bolivar import (
        process_page as process_page,
    )
    from bolivar._bolivar import (
        process_pages as process_pages,
    )
    from bolivar._bolivar import (
        reorder_text_for_output as reorder_text_for_output,
    )
    from bolivar._bolivar import (
        repair_pdf as repair_pdf,
    )
    from bolivar._bolivar import (
        rldecode as rldecode,
    )
    from bolivar._bolivar import (
        safe_cmyk as safe_cmyk,
    )
    from bolivar._bolivar import (
        safe_float as safe_float,
    )
    from bolivar._bolivar import (
        safe_int as safe_int,
    )
    from bolivar._bolivar import (
        safe_matrix as safe_matrix,
    )
    from bolivar._bolivar import (
        safe_rect as safe_rect,
    )
    from bolivar._bolivar import (
        safe_rect_list as safe_rect_list,
    )
    from bolivar._bolivar import (
        safe_rgb as safe_rgb,
    )
    from bolivar._bolivar import (
        shorten_str as shorten_str,
    )
    from bolivar._bolivar import (
        translate_matrix as translate_matrix,
    )
    from bolivar._bolivar import (
        unpad_aes as unpad_aes,
    )

_NATIVE_MODULE: ModuleType | None = None


def load_native_api() -> ModuleType:
    """Load and memoize the native extension module."""
    global _NATIVE_MODULE
    if _NATIVE_MODULE is None:
        _NATIVE_MODULE = import_module("bolivar._bolivar")
    return _NATIVE_MODULE


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
    "PyTableStream",
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
    "extract_pages_async_from_document",
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
