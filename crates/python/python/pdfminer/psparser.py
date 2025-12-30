# pdfminer.psparser compatibility shim


class PSLiteral:
    """PostScript literal name.

    Represents a /Name in PDF syntax.
    """

    def __init__(self, name):
        if isinstance(name, bytes):
            self.name = name
        else:
            self.name = name.encode("latin-1")

    def __repr__(self):
        return f"/{self.name.decode('latin-1', errors='replace')}"

    def __eq__(self, other):
        if isinstance(other, PSLiteral):
            return self.name == other.name
        return False

    def __hash__(self):
        return hash(self.name)


class PSKeyword:
    """PostScript keyword."""

    def __init__(self, name):
        if isinstance(name, bytes):
            self.name = name
        else:
            self.name = name.encode("latin-1")

    def __repr__(self):
        return f"PSKeyword({self.name!r})"


# Keyword singletons (commonly used)
KEYWORD_PROC_BEGIN = PSKeyword(b"{")
KEYWORD_PROC_END = PSKeyword(b"}")
KEYWORD_ARRAY_BEGIN = PSKeyword(b"[")
KEYWORD_ARRAY_END = PSKeyword(b"]")
KEYWORD_DICT_BEGIN = PSKeyword(b"<<")
KEYWORD_DICT_END = PSKeyword(b">>")


class PSSyntaxError(Exception):
    pass


class PSEOF(Exception):
    pass
