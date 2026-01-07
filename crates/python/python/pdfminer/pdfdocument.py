# pdfminer.pdfdocument compatibility shim

from bolivar import PDFDocument as _RustPDFDocument


class XRef:
    """XRef wrapper with trailer dict."""

    def __init__(self, trailer, fallback=False):
        self.trailer = trailer
        self.is_fallback = fallback


class PDFDocument:
    """PDF document container - wraps bolivar's Rust PDFDocument.

    Provides pdfminer.six-compatible API for accessing PDF structure.
    """

    def __init__(self, parser, password=b"", caching=True, fallback=True):
        """Create a PDFDocument from a PDFParser.

        Args:
            parser: PDFParser instance wrapping a file stream
            password: Password for encrypted PDFs (bytes or str)
            caching: Whether to cache resources (ignored, always True)
            fallback: Whether to use fallback parsing (ignored)
        """
        self.parser = parser
        self.caching = caching

        # Convert password to string if bytes
        if isinstance(password, bytes):
            password = password.decode("utf-8", errors="replace")

        # Prefer mmap-backed parsing when a real path is available
        path = None
        if hasattr(parser, "get_path"):
            try:
                path = parser.get_path()
            except Exception:
                path = None

        if path:
            self._rust_doc = _RustPDFDocument.from_path(path, password=password)
        else:
            # Fallback to in-memory bytes
            data = parser.get_data()
            self._rust_doc = _RustPDFDocument(data, password=password)

        # Lazily load pages from Rust
        self._rust_pages = None

        # Compatibility attributes
        self.xrefs = [XRef(t) for t in self._rust_doc.xrefs]
        self.info = self._rust_doc.info  # List of info dicts from Rust
        self.catalog = self._rust_doc.catalog
        self.encryption = None
        self.decipher = None

    def getobj(self, objid):
        """Resolve an indirect object by object id."""
        return self._rust_doc.getobj(objid)

    def page_count(self):
        """Return total number of pages."""
        return self._rust_doc.page_count()

    def page_mediaboxes(self):
        """Return list of mediaboxes for all pages."""
        return [list(box) for box in self._rust_doc.page_mediaboxes()]

    def get_page(self, index):
        """Return a single PDFPage by index."""
        from .pdfpage import PDFPage

        rust_page = self._rust_doc.get_page(index)
        return PDFPage(rust_page, self, page_index=index)


class PDFNoOutlines(Exception):
    pass


class PDFDestinationNotFound(Exception):
    pass
