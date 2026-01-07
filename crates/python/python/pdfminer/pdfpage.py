# pdfminer.pdfpage compatibility shim

from bolivar import PDFPage as _RustPDFPage


class PDFPage:
    """PDF page - wraps bolivar's Rust PDFPage.

    Provides pdfminer.six-compatible API for accessing page properties.
    """

    def __init__(self, rust_page, doc, page_index=None):
        """Create a PDFPage from a Rust PDFPage.

        Args:
            rust_page: bolivar.PDFPage instance from Rust
            doc: Parent PDFDocument (for compatibility)
            page_index: Optional 0-based index for fast lookup
        """
        self._rust_page = rust_page
        self.doc = doc
        self._page_index = page_index
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
        self.resources = rust_page.resources if hasattr(rust_page, "resources") else {}
        self.label = rust_page.label

        # Get annotations from Rust page
        self.annots = rust_page.annots if hasattr(rust_page, "annots") else None
        self.beads = None  # Reading order chain

        # Optional box types - get from Rust if available
        self.bleedbox = (
            list(rust_page.bleedbox)
            if hasattr(rust_page, "bleedbox") and rust_page.bleedbox
            else None
        )
        self.trimbox = (
            list(rust_page.trimbox)
            if hasattr(rust_page, "trimbox") and rust_page.trimbox
            else None
        )
        self.artbox = (
            list(rust_page.artbox)
            if hasattr(rust_page, "artbox") and rust_page.artbox
            else None
        )

        # Populate attrs dict from Rust when available
        if hasattr(rust_page, "attrs"):
            self.attrs = rust_page.attrs
        else:
            self.attrs = {
                "MediaBox": self.mediabox,
                "CropBox": self.cropbox,
                "Rotate": self.rotate,
                "Annots": self.annots,
                "B": self.beads,
            }
            if self.bleedbox:
                self.attrs["BleedBox"] = self.bleedbox
            if self.trimbox:
                self.attrs["TrimBox"] = self.trimbox
            if self.artbox:
                self.attrs["ArtBox"] = self.artbox

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
        try:
            page_count = document._rust_doc.page_count()
        except Exception as e:
            try:
                from pdfplumber.utils.exceptions import PdfminerException
            except Exception:
                raise e
            raise PdfminerException(e)

        if page_count <= 0:
            try:
                from pdfplumber.utils.exceptions import PdfminerException
            except Exception:
                raise RuntimeError("No pages found in PDF")
            raise PdfminerException("No pages found in PDF")

        for idx in range(page_count):
            try:
                rust_page = document._rust_doc.get_page(idx)
            except Exception as e:
                try:
                    from pdfplumber.utils.exceptions import PdfminerException
                except Exception:
                    raise e
                raise PdfminerException(e)
            yield cls(rust_page, document, page_index=idx)

    @classmethod
    def get_pages(
        cls,
        fp,
        page_numbers=None,
        maxpages=0,
        password=b"",
        caching=True,
        check_extractable=True,
    ):
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
