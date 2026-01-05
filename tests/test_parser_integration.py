from io import BytesIO

from pdfminer.pdfparser import PDFParser
from pdfminer.psparser import (
    PSBaseParser,
    PSStackParser,
    PSLiteral,
    PSKeyword,
    LIT,
    KWD,
    KEYWORD_ARRAY_BEGIN,
)


def test_psbaseparser_nexttoken():
    parser = PSBaseParser(BytesIO(b"123 /Name"))
    pos, tok = parser.nexttoken()
    assert tok == 123
    pos, tok = parser.nexttoken()
    assert isinstance(tok, PSLiteral)


def test_psstackparser_nextobject():
    parser = PSStackParser(BytesIO(b"[1 2 3]"))
    pos, obj = parser.nextobject()
    assert obj == [1, 2, 3]


def test_symbol_interning():
    assert LIT("Name") is LIT("Name")
    assert KWD(b"BT") is KWD(b"BT")
    assert isinstance(KEYWORD_ARRAY_BEGIN, PSKeyword)


def test_pdfparser_nextobject_returns_ref():
    parser = PDFParser(BytesIO(b"1 0 R"))
    pos, obj = parser.nextobject()
    assert hasattr(obj, "objid")
    assert obj.objid == 1
