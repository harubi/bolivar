__all__ = [
    "__version__",
    "open",
    "pdfminer",
    "repair",
    "set_debug",
    "utils",
]

import pdfminer
import pdfminer.pdftypes

from . import utils
from ._version import __version__
from .pdf import PDF
from .repair import repair

open = PDF.open
