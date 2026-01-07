# pdfminer.high_level compatibility shim

import os
from typing import Iterable, Optional, Union

from bolivar import (
    extract_pages as _extract_pages,
    extract_pages_from_path as _extract_pages_from_path,
    extract_text as _extract_text,
    extract_text_from_path as _extract_text_from_path,
)


def _read_input(pdf_file):
    if isinstance(pdf_file, (str, os.PathLike)):
        return "path", os.fspath(pdf_file)
    if hasattr(pdf_file, "read"):
        data = pdf_file.read()
        return "bytes", data
    if isinstance(pdf_file, (bytes, bytearray, memoryview)):
        return "bytes", pdf_file
    raise TypeError("pdf_file must be a path, bytes, or file-like object")


def extract_text(
    pdf_file,
    password: str = "",
    page_numbers: Optional[Iterable[int]] = None,
    maxpages: int = 0,
    caching: bool = True,
    laparams=None,
):
    kind, value = _read_input(pdf_file)
    if kind == "path":
        return _extract_text_from_path(
            value, password, page_numbers, maxpages, caching, laparams
        )
    return _extract_text(value, password, page_numbers, maxpages, caching, laparams)


def extract_pages(
    pdf_file,
    password: str = "",
    page_numbers: Optional[Iterable[int]] = None,
    maxpages: int = 0,
    caching: bool = True,
    laparams=None,
):
    kind, value = _read_input(pdf_file)
    if kind == "path":
        pages = _extract_pages_from_path(
            value, password, page_numbers, maxpages, caching, laparams
        )
    else:
        pages = _extract_pages(
            value, password, page_numbers, maxpages, caching, laparams
        )
    return iter(pages)
