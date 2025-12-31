# pdfminer.pdfpage compatibility shim

from bolivar import PDFPage as _RustPDFPage


class PDFPage:
    """PDF page - wraps bolivar's Rust PDFPage.

    Provides pdfminer.six-compatible API for accessing page properties.
    """

    def __init__(self, rust_page, doc):
        """Create a PDFPage from a Rust PDFPage.

        Args:
            rust_page: bolivar.PDFPage instance from Rust
            doc: Parent PDFDocument (for compatibility)
        """
        self._rust_page = rust_page
        self.doc = doc
        self.pageid = rust_page.pageid

        # Convert mediabox tuple to list (pdfminer.six uses lists)
        if rust_page.mediabox:
            self.mediabox = list(rust_page.mediabox)
        else:
            self.mediabox = None

        # Convert cropbox tuple to list
        if rust_page.cropbox:
            self.cropbox = list(rust_page.cropbox)
        else:
            self.cropbox = self.mediabox  # Default to mediabox

        self.rotate = rust_page.rotate
        self.resources = {}  # TODO: Populate from Rust if needed
        self.label = rust_page.label

        # Annotations - TODO: Expose from Rust PDFPage
        # For now, return None (pdfplumber handles this gracefully)
        self.annots = None
        self.beads = None  # Reading order chain

        # Populate attrs dict with uppercase keys for pdfplumber compatibility
        # pdfplumber accesses page_obj.attrs.get("MediaBox") etc.
        self.attrs = {
            "MediaBox": self.mediabox,
            "CropBox": self.cropbox,
            "Rotate": self.rotate,
            "Annots": self.annots,
            "B": self.beads,
        }

    @classmethod
    def create_pages(cls, document, caching=True, check_extractable=True):
        """Iterate over pages in a PDF document.

        Args:
            document: PDFDocument instance
            caching: Whether to cache resources (ignored)
            check_extractable: Whether to check extractability (ignored)

        Yields:
            PDFPage instances for each page in the document
        """
        # Get pages from the Rust document stored in PDFDocument
        for rust_page in document._rust_pages:
            yield cls(rust_page, document)

    @classmethod
    def get_pages(cls, fp, page_numbers=None, maxpages=0, password=b"",
                  caching=True, check_extractable=True):
        """Legacy interface for iterating pages.

        This is a convenience method that creates parser and document.
        """
        from .pdfparser import PDFParser
        from .pdfdocument import PDFDocument

        parser = PDFParser(fp)
        doc = PDFDocument(parser, password=password, caching=caching)

        for i, page in enumerate(cls.create_pages(doc)):
            if page_numbers is not None and i not in page_numbers:
                continue
            if maxpages > 0 and i >= maxpages:
                break
            yield page
