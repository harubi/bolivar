# pdfminer.pdfdocument compatibility shim
#
# TODO: Replace with actual bolivar bindings

class PDFDocument:
    """PDF document container - stub for pdfplumber compatibility."""

    def __init__(self, parser, password=b"", caching=True, fallback=True):
        self.parser = parser
        self.caching = caching
        self.xrefs = []
        self.info = []
        self.catalog = {}
        self._pages = None
        self.encryption = None
        self.decipher = None
        # TODO: Delegate to bolivar-core
        raise NotImplementedError("PDFDocument not yet implemented via bolivar")


class PDFNoOutlines(Exception):
    pass


class PDFDestinationNotFound(Exception):
    pass
