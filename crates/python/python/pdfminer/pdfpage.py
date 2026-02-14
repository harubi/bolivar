# pdfminer.pdfpage compatibility shim
from __future__ import annotations

from typing import TYPE_CHECKING, Any, BinaryIO, Protocol, cast

if TYPE_CHECKING:
    from collections.abc import Container, Generator

    from bolivar._bolivar import PDFDocument as _NativePDFDocument
    from bolivar._bolivar import PDFPage as _NativePDFPage


class _RustPageLike(Protocol):
    pageid: int
    mediabox: tuple[float, float, float, float] | None
    cropbox: tuple[float, float, float, float] | None
    rotate: int
    resources: dict[str, Any]
    label: str | None
    annots: list[Any]
    bleedbox: tuple[float, float, float, float] | None
    trimbox: tuple[float, float, float, float] | None
    artbox: tuple[float, float, float, float] | None
    attrs: dict[str, Any]


class _DocumentLike(Protocol):
    _rust_doc: _NativePDFDocument


class PDFPage:
    """PDF page - wraps bolivar's Rust PDFPage.

    Provides pdfminer.six-compatible API for accessing page properties.
    """

    _rust_page: _NativePDFPage
    doc: _DocumentLike
    _page_index: int | None
    pageid: int
    mediabox: list[float] | None
    cropbox: list[float] | None
    rotate: int
    resources: dict[str, Any]
    label: str | None
    annots: list[Any]
    beads: object | None
    bleedbox: list[float] | None
    trimbox: list[float] | None
    artbox: list[float] | None
    attrs: dict[str, Any]

    def __init__(
        self,
        rust_page: _RustPageLike,
        doc: _DocumentLike,
        page_index: int | None = None,
    ) -> None:
        """Create a PDFPage from a Rust PDFPage.

        Args:
            rust_page: bolivar.PDFPage instance from Rust
            doc: Parent PDFDocument (for compatibility)
            page_index: Optional 0-based index for fast lookup
        """
        self._rust_page: _NativePDFPage = cast("_NativePDFPage", rust_page)
        self.doc: _DocumentLike = doc
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
        self.resources = rust_page.resources
        self.label = rust_page.label

        # Get annotations from Rust page
        self.annots = rust_page.annots
        self.beads = None  # Reading order chain

        # Optional box types - get from Rust if available
        self.bleedbox = list(rust_page.bleedbox) if rust_page.bleedbox else None
        self.trimbox = list(rust_page.trimbox) if rust_page.trimbox else None
        self.artbox = list(rust_page.artbox) if rust_page.artbox else None

        # Populate attrs dict from Rust (single source of truth)
        self.attrs = rust_page.attrs

    @classmethod
    def create_pages(
        cls,
        document: _DocumentLike,
        caching: bool = True,
        check_extractable: bool = True,
    ) -> Generator[PDFPage, None, None]:
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
        except Exception as exc:
            try:
                from pdfplumber.utils.exceptions import PdfminerException
            except Exception:
                raise exc from exc
            raise PdfminerException(exc) from exc

        if page_count <= 0:
            try:
                from pdfplumber.utils.exceptions import PdfminerException
            except Exception as exc:
                raise RuntimeError("No pages found in PDF") from exc
            raise PdfminerException("No pages found in PDF")

        for idx in range(page_count):
            try:
                rust_page = document._rust_doc.get_page(idx)
            except Exception as exc:
                try:
                    from pdfplumber.utils.exceptions import PdfminerException
                except Exception:
                    raise exc from exc
                raise PdfminerException(exc) from exc
            yield cls(rust_page, document, page_index=idx)

    @classmethod
    def get_pages(
        cls,
        fp: BinaryIO | bytes | bytearray,
        page_numbers: Container[int] | None = None,
        maxpages: int = 0,
        password: bytes | str = b"",
        caching: bool = True,
        check_extractable: bool = True,
    ) -> Generator[PDFPage, None, None]:
        """Legacy interface for iterating pages.

        This is a convenience method that creates parser and document.
        """
        from .pdfdocument import PDFDocument
        from .pdfparser import PDFParser

        parser = PDFParser(fp)
        doc = PDFDocument(parser, password=password, caching=caching)

        for i, page in enumerate(cls.create_pages(doc)):
            if page_numbers is not None and i not in page_numbers:
                continue
            if maxpages > 0 and i >= maxpages:
                break
            yield page
