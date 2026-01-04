# pdfminer.pdfparser compatibility shim

import os

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
        self._path = None
        self._data = None

        # Remember start position if possible (for compatibility)
        self._start_pos = None
        if hasattr(fp, "tell"):
            try:
                self._start_pos = fp.tell()
            except Exception:
                self._start_pos = None

        # If fp is a path-like, use it directly for mmap-backed parsing
        if isinstance(fp, (str, os.PathLike)):
            self._path = os.fspath(fp)
        elif isinstance(fp, (bytes, bytearray, memoryview)):
            self._data = fp
        else:
            # If fp has a real filesystem path, prefer that (mmap)
            path = getattr(fp, "name", None)
            if isinstance(path, str) and os.path.isfile(path):
                self._path = path

        # Fallback: read bytes into memory
        if self._path is None:
            if self._data is None:
                self._data = self._read_all_bytes()

    def _read_all_bytes(self) -> bytes:
        if not hasattr(self.fp, "read"):
            return b""
        data = self.fp.read()
        # Try to restore position for compatibility
        if self._start_pos is not None and hasattr(self.fp, "seek"):
            try:
                self.fp.seek(self._start_pos)
            except Exception:
                pass
        return data

    def get_data(self) -> bytes:
        """Get the PDF data as bytes."""
        if self._data is None:
            if self._path is not None:
                with open(self._path, "rb") as fh:
                    self._data = fh.read()
            else:
                self._data = self._read_all_bytes()
        return self._data

    def get_path(self):
        """Get the PDF path if available (preferred for mmap)."""
        return self._path


# pdfminer.six uses a PSKeyword for null; our objects resolve to None
PDFParser.KEYWORD_NULL = None
