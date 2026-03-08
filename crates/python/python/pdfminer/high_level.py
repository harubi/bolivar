# pdfminer.high_level compatibility shim

from __future__ import annotations

import logging
import io
import os
import sys
from typing import TYPE_CHECKING, BinaryIO

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
from .pdfdevice import TagExtractor
from .pdfexceptions import PDFValueError
from .pdfinterp import PDFPageInterpreter, PDFResourceManager
from .pdfpage import PDFPage

if TYPE_CHECKING:
    from collections.abc import Generator, Iterable

    from bolivar._bolivar import LAParams

    from .utils import AnyIO

PDFInput = str | os.PathLike[str] | bytes | bytearray | memoryview | BinaryIO


def _resolve_input(pdf_file: PDFInput) -> str | bytes | bytearray:
    if isinstance(pdf_file, (str, os.PathLike)):
        return os.fspath(pdf_file)
    if isinstance(pdf_file, bytes):
        return pdf_file
    if isinstance(pdf_file, bytearray):
        return pdf_file
    if isinstance(pdf_file, memoryview):
        return bytes(pdf_file)
    if hasattr(pdf_file, "read"):
        return pdf_file.read()
    raise TypeError("pdf_file must be a path, bytes, or file-like object")


def _normalize_page_numbers(
    page_numbers: Iterable[int] | None,
) -> list[int] | None:
    if page_numbers is None:
        return None
    selected_pages = list(page_numbers)
    if not selected_pages:
        return None
    return selected_pages


def _is_binary_output(outfp: object) -> bool:
    mode = getattr(outfp, "mode", "")
    if "b" in mode:
        return True
    if hasattr(outfp, "mode"):
        return False
    if isinstance(outfp, io.BytesIO):
        return True
    return not isinstance(outfp, (io.StringIO, io.TextIOBase))


class _TextIOBridge:
    def __init__(self, outfp: AnyIO, codec: str) -> None:
        self._outfp = outfp
        self._codec = codec

    def write(self, data: bytes | str) -> object:
        if isinstance(data, bytes):
            return self._outfp.write(data.decode(self._codec))
        return self._outfp.write(data)

    def flush(self) -> object:
        flush = getattr(self._outfp, "flush", None)
        if callable(flush):
            return flush()
        return None


def _prepare_converter_output(
    outfp: AnyIO,
    output_type: str,
    codec: str | None,
) -> tuple[AnyIO, str | None]:
    binary_output = _is_binary_output(outfp)

    if output_type == "text":
        effective_codec = codec or "utf-8"
        if binary_output:
            return outfp, effective_codec
        return _TextIOBridge(outfp, effective_codec), effective_codec

    if output_type == "xml":
        if codec is None:
            if binary_output:
                raise PDFValueError("Codec is required for a binary I/O output")
            return _TextIOBridge(outfp, "utf-8"), ""
        if not binary_output:
            raise PDFValueError("Codec is required for a binary I/O output")
        return outfp, codec

    if output_type == "html":
        if codec is None:
            if binary_output:
                raise PDFValueError("Codec is required for a binary I/O output")
            return _TextIOBridge(outfp, "utf-8"), ""
        if not binary_output:
            raise PDFValueError("Codec must not be specified for a text I/O output")
        return outfp, codec

    return outfp, codec


def extract_text_to_fp(
    inf: BinaryIO,
    outfp: AnyIO,
    output_type: str = "text",
    codec: str | None = "utf-8",
    laparams: LAParams | None = None,
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

    if output_type != "text" and outfp == sys.stdout:
        outfp = sys.stdout.buffer

    page_numbers = _normalize_page_numbers(page_numbers)
    converter_outfp, effective_codec = _prepare_converter_output(
        outfp, output_type, codec
    )

    if output_type in {"tag", "text"}:
        rsrcmgr = PDFResourceManager(caching=not disable_caching)
        if output_type == "text":
            device = TextConverter(
                rsrcmgr,
                converter_outfp,
                codec=effective_codec if effective_codec is not None else "utf-8",
                laparams=laparams,
                imagewriter=imagewriter,
            )
        else:
            device = TagExtractor(
                rsrcmgr,
                converter_outfp,
                codec=effective_codec if effective_codec is not None else "utf-8",
            )
        interpreter = PDFPageInterpreter(rsrcmgr, device)
        for page in PDFPage.get_pages(
            inf,
            page_numbers,
            maxpages=maxpages,
            password=password,
            caching=not disable_caching,
        ):
            page.rotate = (page.rotate + rotation) % 360
            interpreter.process_page(page)
        device.close()
        return

    rsrcmgr = PDFResourceManager(caching=not disable_caching)

    if output_type == "xml":
        device = XMLConverter(
            rsrcmgr,
            converter_outfp,
            codec=effective_codec if effective_codec is not None else "utf-8",
            laparams=laparams,
            stripcontrol=strip_control,
            imagewriter=imagewriter,
        )
    elif output_type == "html":
        device = HTMLConverter(
            rsrcmgr,
            converter_outfp,
            codec=effective_codec if effective_codec is not None else "utf-8",
            scale=scale,
            layoutmode=layoutmode,
            laparams=laparams,
            imagewriter=imagewriter,
        )
    elif output_type == "hocr":
        device = HOCRConverter(
            rsrcmgr,
            converter_outfp,
            codec=effective_codec if effective_codec is not None else "utf-8",
            laparams=laparams,
            stripcontrol=strip_control,
            imagewriter=imagewriter,
        )
    else:
        msg = f"Output type can be text, html, xml or tag but is {output_type}"
        raise PDFValueError(msg)

    resolved = _resolve_input(inf)
    if output_dir:
        if isinstance(resolved, str):
            pages = _extract_pages_with_images_from_path(
                resolved,
                output_dir,
                password,
                page_numbers,
                maxpages,
                not disable_caching,
                laparams,
                rotation,
            )
        else:
            pages = _extract_pages_with_images(
                resolved,
                output_dir,
                password,
                page_numbers,
                maxpages,
                not disable_caching,
                laparams,
                rotation,
            )
    else:
        if isinstance(resolved, str):
            pages = _extract_pages_from_path(
                resolved,
                password,
                page_numbers,
                maxpages,
                not disable_caching,
                laparams,
                rotation,
            )
        else:
            pages = _extract_pages(
                resolved,
                password,
                page_numbers,
                maxpages,
                not disable_caching,
                laparams,
                rotation,
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
    codec: str = "utf-8",
    laparams: LAParams | None = None,
) -> str:
    pages_list = _normalize_page_numbers(page_numbers)
    resolved = _resolve_input(pdf_file)
    if isinstance(resolved, str):
        return _extract_text_from_path(
            resolved, password, pages_list, maxpages, caching, laparams
        )
    return _extract_text(
        resolved,
        password,
        pages_list,
        maxpages,
        caching,
        laparams,
    )


def extract_pages(
    pdf_file: PDFInput,
    password: str = "",
    page_numbers: Iterable[int] | None = None,
    maxpages: int = 0,
    caching: bool = True,
    laparams: LAParams | None = None,
) -> Generator[LTPage, None, None]:
    pages_list = _normalize_page_numbers(page_numbers)
    resolved = _resolve_input(pdf_file)
    if isinstance(resolved, str):
        pages = _extract_pages_from_path(
            resolved, password, pages_list, maxpages, caching, laparams
        )
    else:
        pages = _extract_pages(
            resolved,
            password,
            pages_list,
            maxpages,
            caching,
            laparams,
        )
    return (LTPage(page) for page in pages)
