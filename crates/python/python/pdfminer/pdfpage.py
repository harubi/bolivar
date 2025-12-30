# pdfminer.pdfpage compatibility shim

class PDFPage:
    """PDF page - stub for pdfplumber compatibility."""

    def __init__(self, doc, pageid, attrs):
        self.doc = doc
        self.pageid = pageid
        self.attrs = attrs
        self.mediabox = None
        self.cropbox = None
        self.rotate = 0
        self.resources = {}

    @classmethod
    def create_pages(cls, document, caching=True, check_extractable=True):
        """Iterate over pages in a PDF document."""
        # TODO: Delegate to bolivar-core
        raise NotImplementedError("PDFPage.create_pages not yet implemented via bolivar")

    @classmethod
    def get_pages(cls, fp, page_numbers=None, maxpages=0, password=b"",
                  caching=True, check_extractable=True):
        """Legacy interface for iterating pages."""
        raise NotImplementedError("PDFPage.get_pages not yet implemented via bolivar")
