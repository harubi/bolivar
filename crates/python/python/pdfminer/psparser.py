# pdfminer.psparser compatibility shim (Rust-backed)

import pdfminer.psexceptions as psexceptions
from bolivar._native_api import KWD, PSKeyword, PSLiteral

PSException = psexceptions.PSException
PSEOF = psexceptions.PSEOF
PSSyntaxError = psexceptions.PSSyntaxError
PSTypeError = psexceptions.PSTypeError
PSValueError = psexceptions.PSValueError

KEYWORD_PROC_BEGIN = KWD(b"{")
KEYWORD_PROC_END = KWD(b"}")
KEYWORD_ARRAY_BEGIN = KWD(b"[")
KEYWORD_ARRAY_END = KWD(b"]")
KEYWORD_DICT_BEGIN = KWD(b"<<")
KEYWORD_DICT_END = KWD(b">>")


def literal_name(x: object) -> str:
    if isinstance(x, PSLiteral):
        name = x.name
        if isinstance(name, str):
            return name
        try:
            return bytes(name).decode("utf-8")
        except UnicodeDecodeError:
            return str(name)
    return str(x)


def keyword_name(x: object) -> object:
    if not isinstance(x, PSKeyword):
        return x
    name = x.name
    if isinstance(name, bytes):
        return name.decode("utf-8", "ignore")
    return str(name)
