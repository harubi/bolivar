# pdfminer.pdfdocument compatibility shim
from __future__ import annotations

from typing import TYPE_CHECKING, Any, Protocol

from bolivar import PDFDocument as _RustPDFDocument

if TYPE_CHECKING:
    from collections.abc import Iterable, Iterator

    from bolivar._bolivar import PDFDocument as _NativePDFDocument

from .pdfexceptions import PDFException, PDFObjectNotFound


class _ParserLike(Protocol):
    def get_path(self) -> str | None:
        """Return source path if available."""

    def get_data(self) -> bytes:
        """Return source bytes."""


class XRef:
    """XRef wrapper with trailer dict."""

    def __init__(
        self,
        trailer: dict[object, object],
        objids: Iterable[int] | None = None,
        fallback: bool = False,
    ) -> None:
        self.trailer = trailer
        self._objids = list(objids) if objids is not None else []
        self.is_fallback = fallback

    def get_trailer(self) -> dict[object, object]:
        return self.trailer

    def get_objids(self) -> list[int]:
        return self._objids


class PDFDocument:
    """PDF document container - wraps bolivar's Rust PDFDocument.

    Provides pdfminer.six-compatible API for accessing PDF structure.
    """

    _rust_doc: _NativePDFDocument
    parser: _ParserLike
    caching: bool
    _rust_pages: object | None
    xrefs: list[XRef]
    info: list[dict[str, Any]]
    catalog: dict[str, Any]
    encryption: object | None
    decipher: object | None

    def __init__(
        self,
        parser: _ParserLike,
        password: bytes | str = b"",
        caching: bool = True,
        fallback: bool = True,
    ) -> None:
        """Create a PDFDocument from a PDFParser.

        Args:
            parser: PDFParser instance wrapping a file stream
            password: Password for encrypted PDFs (bytes or str)
            caching: Whether to cache resolved objects (default: True)
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
            self._rust_doc = _RustPDFDocument.from_path(
                path, password=password, caching=caching
            )
        else:
            # Fallback to in-memory bytes
            data = parser.get_data()
            self._rust_doc = _RustPDFDocument(data, password=password, caching=caching)

        # Lazily load pages from Rust
        self._rust_pages = None

        # Compatibility attributes
        trailers = self._rust_doc.xrefs
        objids = self._rust_doc.xref_objids
        fallbacks = self._rust_doc.xref_fallbacks
        self.xrefs = [
            PDFXRefFallback(trailer=t, objids=o) if fb else XRef(t, o, fallback=False)
            for t, o, fb in zip(trailers, objids, fallbacks, strict=False)
        ]
        self.info = self._rust_doc.info  # List of info dicts from Rust
        self.catalog = self._rust_doc.catalog
        self.encryption = None
        self.decipher = None

    def getobj(self, objid: int) -> object:
        """Resolve an indirect object by object id."""
        try:
            return self._rust_doc.getobj(objid)
        except Exception as exc:
            raise PDFObjectNotFound(objid) from exc

    def get_page_labels(self) -> Iterator[object]:
        """Return an iterator over page labels."""
        try:
            labels = self._rust_doc.get_page_labels()
        except Exception as exc:
            raise PDFNoPageLabels() from exc
        return iter(labels)

    def page_count(self) -> int:
        """Return total number of pages."""
        return self._rust_doc.page_count()

    def page_mediaboxes(self) -> list[list[float]]:
        """Return list of mediaboxes for all pages."""
        return [list(box) for box in self._rust_doc.page_mediaboxes()]

    def get_page(self, index: int) -> object:
        """Return a single PDFPage by index."""
        from .pdfpage import PDFPage

        rust_page = self._rust_doc.get_page(index)
        return PDFPage(rust_page, self, page_index=index)


class PDFNoOutlines(PDFException):
    pass


class PDFNoPageLabels(PDFException):
    pass


class PDFDestinationNotFound(PDFException):
    pass


class PDFXRefFallback(XRef):
    """Fallback xref used when standard xref parsing fails."""

    def __init__(
        self,
        trailer: dict[object, object] | None = None,
        objids: Iterable[int] | None = None,
    ) -> None:
        super().__init__(trailer or {}, objids=objids, fallback=True)
        self.offsets: dict[int, int] = {}

    def __repr__(self) -> str:
        return f"<PDFXRefFallback: offsets={self.offsets.keys()!r}>"
