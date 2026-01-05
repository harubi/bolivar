# pdfminer.psparser compatibility shim (Rust-backed)

from bolivar._bolivar import (
    PSBaseParser,
    PSStackParser,
    PSLiteral,
    PSKeyword,
    LIT,
    KWD,
    KEYWORD_PROC_BEGIN,
    KEYWORD_PROC_END,
    KEYWORD_ARRAY_BEGIN,
    KEYWORD_ARRAY_END,
    KEYWORD_DICT_BEGIN,
    KEYWORD_DICT_END,
)

from . import psexceptions

PSException = psexceptions.PSException
PSEOF = psexceptions.PSEOF
PSSyntaxError = psexceptions.PSSyntaxError
PSTypeError = psexceptions.PSTypeError
PSValueError = psexceptions.PSValueError


def literal_name(x):
    if isinstance(x, PSLiteral):
        name = x.name
        if isinstance(name, str):
            return name
        try:
            return bytes(name).decode("utf-8")
        except UnicodeDecodeError:
            return str(name)
    return str(x)


def keyword_name(x):
    if not isinstance(x, PSKeyword):
        return x
    name = x.name
    if isinstance(name, bytes):
        return name.decode("utf-8", "ignore")
    return str(name)
