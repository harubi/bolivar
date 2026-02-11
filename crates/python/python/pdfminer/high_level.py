# pdfminer.high_level compatibility shim

import logging
import os
import sys
from collections.abc import Generator, Iterable
from typing import BinaryIO, Literal, cast

from bolivar import (
    extract_pages as _extract_pages,
)
from bolivar import (
    extract_pages_from_path as _extract_pages_from_path,
)
from bolivar import (
    extract_pages_with_images as _extract_pages_with_images,
)
from bolivar import (
    extract_pages_with_images_from_path as _extract_pages_with_images_from_path,
)
from bolivar import (
    extract_text as _extract_text,
)
from bolivar import (
    extract_text_from_path as _extract_text_from_path,
)

from .converter import HOCRConverter, HTMLConverter, TextConverter, XMLConverter
from .image import ImageWriter
from .layout import LTPage
from .pdfexceptions import PDFValueError
from .pdfinterp import PDFResourceManager
from .utils import AnyIO

PDFInput = str | os.PathLike[str] | bytes | bytearray | memoryview | BinaryIO
ReadInput = (
    tuple[Literal["path"], str | bytes]
    | tuple[Literal["bytes"], bytes | bytearray | memoryview]
)


def _read_input(pdf_file: PDFInput) -> ReadInput:
    if isinstance(pdf_file, (str, os.PathLike)):
        return "path", cast("str | bytes", os.fspath(pdf_file))
    if isinstance(pdf_file, (bytes, bytearray, memoryview)):
        return "bytes", pdf_file
    if hasattr(pdf_file, "read"):
        data = cast("bytes | bytearray | memoryview", pdf_file.read())
        return "bytes", data
    raise TypeError("pdf_file must be a path, bytes, or file-like object")


def extract_text_to_fp(
    inf: BinaryIO,
    outfp: AnyIO,
    output_type: str = "text",
    codec: str = "utf-8",
    laparams: object | None = None,
    maxpages: int = 0,
    page_numbers: Iterable[int] | None = None,
    password: str = "",
    scale: float = 1.0,
    rotation: int = 0,
    layoutmode: str = "normal",
    output_dir: str | None = None,
    strip_control: bool = False,
    debug: bool = False,
    disable_caching: bool = False,
    **_kwargs: object,
) -> None:
    if debug:
        logging.getLogger().setLevel(logging.DEBUG)

    imagewriter = None
    if output_dir:
        imagewriter = ImageWriter(output_dir)

    rsrcmgr = PDFResourceManager(caching=not disable_caching)

    if output_type != "text" and outfp == sys.stdout:
        outfp = sys.stdout.buffer

    if output_type == "text":
        device = TextConverter(
            rsrcmgr,
            outfp,
            codec=codec,
            laparams=laparams,
            imagewriter=imagewriter,
        )
    elif output_type == "xml":
        device = XMLConverter(
            rsrcmgr,
            outfp,
            codec=codec,
            laparams=laparams,
            stripcontrol=strip_control,
            imagewriter=imagewriter,
        )
    elif output_type == "html":
        device = HTMLConverter(
            rsrcmgr,
            outfp,
            codec=codec,
            scale=scale,
            layoutmode=layoutmode,
            laparams=laparams,
            imagewriter=imagewriter,
        )
    elif output_type == "hocr":
        device = HOCRConverter(
            rsrcmgr,
            outfp,
            codec=codec,
            laparams=laparams,
            stripcontrol=strip_control,
            imagewriter=imagewriter,
        )
    elif output_type == "tag":
        device = TextConverter(
            rsrcmgr,
            outfp,
            codec=codec,
            laparams=laparams,
            imagewriter=imagewriter,
        )
    else:
        msg = f"Output type can be text, html, xml or tag but is {output_type}"
        raise PDFValueError(msg)

    if page_numbers is not None:
        page_numbers = list(page_numbers)

    kind, value = _read_input(inf)
    if output_dir:
        if kind == "path":
            pages = _extract_pages_with_images_from_path(
                value,
                output_dir,
                password,
                page_numbers,
                maxpages,
                not disable_caching,
                laparams,
            )
        else:
            pages = _extract_pages_with_images(
                value,
                output_dir,
                password,
                page_numbers,
                maxpages,
                not disable_caching,
                laparams,
            )
    else:
        if kind == "path":
            pages = _extract_pages_from_path(
                value, password, page_numbers, maxpages, not disable_caching, laparams
            )
        else:
            pages = _extract_pages(
                value, password, page_numbers, maxpages, not disable_caching, laparams
            )

    for page in pages:
        device._receive_layout(page)

    device.close()


def extract_text(
    pdf_file: PDFInput,
    password: str = "",
    page_numbers: Iterable[int] | None = None,
    maxpages: int = 0,
    caching: bool = True,
    laparams: object | None = None,
) -> str:
    kind, value = _read_input(pdf_file)
    if kind == "path":
        return _extract_text_from_path(
            value, password, page_numbers, maxpages, caching, laparams
        )
    return _extract_text(value, password, page_numbers, maxpages, caching, laparams)


def extract_pages(
    pdf_file: PDFInput,
    password: str = "",
    page_numbers: Iterable[int] | None = None,
    maxpages: int = 0,
    caching: bool = True,
    laparams: object | None = None,
) -> Generator[LTPage, None, None]:
    kind, value = _read_input(pdf_file)
    if kind == "path":
        pages = _extract_pages_from_path(
            value, password, page_numbers, maxpages, caching, laparams
        )
    else:
        pages = _extract_pages(
            value, password, page_numbers, maxpages, caching, laparams
        )
    return (LTPage(page) for page in pages)
