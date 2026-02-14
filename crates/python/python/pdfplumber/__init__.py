__all__ = [
    "__version__",
    "open",
    "pdfminer",
    "repair",
    "set_debug",
    "utils",
]

from io import BufferedReader, BytesIO
from pathlib import Path
from typing import Any, Literal

import pdfminer
import pdfminer.pdftypes

from . import utils
from ._version import __version__
from .pdf import PDF
from .repair import T_repair_setting, repair


def open(
    path_or_fp: str | Path | BufferedReader | BytesIO,
    pages: list[int] | tuple[int] | None = None,
    laparams: dict[str, Any] | None = None,
    password: str | None = None,
    strict_metadata: bool = False,
    unicode_norm: Literal["NFC", "NFKC", "NFD", "NFKD"] | None = None,
    repair: bool = False,
    gs_path: str | Path | None = None,
    repair_setting: T_repair_setting = "default",
    raise_unicode_errors: bool = True,
) -> PDF:
    return PDF.open(
        path_or_fp=path_or_fp,
        pages=pages,
        laparams=laparams,
        password=password,
        strict_metadata=strict_metadata,
        unicode_norm=unicode_norm,
        repair=repair,
        gs_path=gs_path,
        repair_setting=repair_setting,
        raise_unicode_errors=raise_unicode_errors,
    )
