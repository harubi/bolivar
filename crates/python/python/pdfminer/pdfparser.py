# pdfminer.pdfparser compatibility shim

# Re-export PDFObjRef for compatibility (pdfplumber imports it from here)
from .pdftypes import PDFObjRef


class PDFParser:
    """PDF parser - wraps a file stream for PDF parsing.

    In the pdfminer.six API, PDFParser wraps a file stream and is then
    passed to PDFDocument. In bolivar, PDFDocument takes bytes directly,
    so this class just stores the stream for later reading.
    """

    def __init__(self, fp):
        """Create a PDFParser from a file-like object.

        Args:
            fp: A file-like object opened in binary mode (rb)
        """
        self.fp = fp
        # Read all bytes - bolivar needs the full content
        # Store current position to restore after reading
        start_pos = fp.tell() if hasattr(fp, 'tell') else 0
        self._data = fp.read()
        # Try to restore position for compatibility
        if hasattr(fp, 'seek'):
            try:
                fp.seek(start_pos)
            except Exception:
                pass

    def get_data(self) -> bytes:
        """Get the PDF data as bytes."""
        return self._data
