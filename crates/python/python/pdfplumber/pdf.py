import itertools
import logging
import pathlib
from collections.abc import Generator
from io import BufferedReader, BytesIO
from types import TracebackType
from typing import Any, ClassVar, Literal

from pdfminer.layout import LAParams
from pdfminer.pdfdocument import PDFDocument
from pdfminer.pdfinterp import PDFResourceManager
from pdfminer.pdfpage import PDFPage
from pdfminer.pdfparser import PDFParser

from ._typing import StructElementDict, T_num, T_obj_list
from .container import Container
from .page import Page
from .repair import T_repair_setting, _repair
from .structure import PDFStructTree, StructTreeMissing
from .utils import resolve_and_decode
from .utils.exceptions import PdfminerException

logger = logging.getLogger(__name__)


class PDF(Container):
    cached_properties: ClassVar[list[str]] = [*Container.cached_properties, "_pages"]

    def __init__(
        self,
        stream: BufferedReader | BytesIO,
        stream_is_external: bool = False,
        path: pathlib.Path | None = None,
        pages: list[int] | tuple[int] | None = None,
        laparams: dict[str, Any] | None = None,
        password: str | None = None,
        strict_metadata: bool = False,
        unicode_norm: Literal["NFC", "NFKC", "NFD", "NFKD"] | None = None,
        raise_unicode_errors: bool = True,
    ) -> None:
        self.stream = stream
        self.stream_is_external = stream_is_external
        self.path = path
        self.pages_to_parse = pages
        self.laparams = None if laparams is None else LAParams(**laparams)
        self.password = password
        self.unicode_norm = unicode_norm
        self.raise_unicode_errors = raise_unicode_errors

        try:
            self.doc = PDFDocument(PDFParser(stream), password=password or "")
        except Exception as e:
            raise PdfminerException(e) from e
        self.rsrcmgr = PDFResourceManager()
        self.metadata = {}

        for info in self.doc.info:
            self.metadata.update(info)
        for k, v in self.metadata.items():
            try:
                self.metadata[k] = resolve_and_decode(v)
            except Exception as e:  # pragma: nocover
                if strict_metadata:
                    # Raise an exception since unable to resolve the metadata value.
                    raise
                # This metadata value could not be parsed. Instead of failing the PDF
                # read, treat it as a warning only if `strict_metadata=False`.
                logger.warning(
                    f'[WARNING] Metadata key "{k}" could not be parsed due to '
                    f"exception: {e!s}"
                )

    @classmethod
    def open(
        cls,
        path_or_fp: str | pathlib.Path | BufferedReader | BytesIO,
        pages: list[int] | tuple[int] | None = None,
        laparams: dict[str, Any] | None = None,
        password: str | None = None,
        strict_metadata: bool = False,
        unicode_norm: Literal["NFC", "NFKC", "NFD", "NFKD"] | None = None,
        repair: bool = False,
        gs_path: str | pathlib.Path | None = None,
        repair_setting: T_repair_setting = "default",
        raise_unicode_errors: bool = True,
    ) -> "PDF":
        stream: BufferedReader | BytesIO

        if repair:
            stream = _repair(
                path_or_fp, password=password, gs_path=gs_path, setting=repair_setting
            )
            stream_is_external = False
            # Although the original file has a path,
            # the repaired version does not
            path = None
        elif isinstance(path_or_fp, (str, pathlib.Path)):
            stream = open(path_or_fp, "rb")  # noqa: SIM115
            stream_is_external = False
            path = pathlib.Path(path_or_fp)
        else:
            stream = path_or_fp
            stream_is_external = True
            path = None

        try:
            return cls(
                stream,
                path=path,
                pages=pages,
                laparams=laparams,
                password=password,
                strict_metadata=strict_metadata,
                unicode_norm=unicode_norm,
                stream_is_external=stream_is_external,
                raise_unicode_errors=raise_unicode_errors,
            )

        except PdfminerException:
            if not stream_is_external:
                stream.close()
            raise

    def close(self) -> None:
        self.flush_cache()

        for page in self.pages:
            page.close()

        if not self.stream_is_external:
            self.stream.close()

    def __enter__(self) -> "PDF":
        return self

    def __exit__(
        self,
        t: type[BaseException] | None,
        value: BaseException | None,
        traceback: TracebackType | None,
    ) -> None:
        self.close()

    @property
    def pages(self) -> list[Page]:
        if hasattr(self, "_pages"):
            return self._pages

        doctop: T_num = 0
        pp = self.pages_to_parse
        self._pages: list[Page] = []

        def iter_pages() -> Generator[PDFPage, None, None]:
            gen = PDFPage.create_pages(self.doc)
            while True:
                try:
                    yield next(gen)
                except StopIteration:
                    break
                except Exception as e:
                    raise PdfminerException(e) from e

        for i, page in enumerate(iter_pages()):
            page_number = i + 1
            if pp is not None and page_number not in pp:
                continue
            p = Page(self, page, page_number=page_number, initial_doctop=doctop)
            self._pages.append(p)
            doctop += p.height
        return self._pages

    @property
    def objects(self) -> dict[str, T_obj_list]:
        if hasattr(self, "_objects"):
            return self._objects
        all_objects: dict[str, T_obj_list] = {}
        for p in self.pages:
            for kind in p.objects:
                all_objects[kind] = all_objects.get(kind, []) + p.objects[kind]
        self._objects: dict[str, T_obj_list] = all_objects
        return self._objects

    @property
    def annots(self) -> list[dict[str, Any]]:
        gen = (p.annots for p in self.pages)
        return list(itertools.chain(*gen))

    @property
    def hyperlinks(self) -> list[dict[str, Any]]:
        gen = (p.hyperlinks for p in self.pages)
        return list(itertools.chain(*gen))

    @property
    def structure_tree(self) -> list[dict[str, Any] | StructElementDict]:
        """Return the structure tree for the document."""
        try:
            return [elem.to_dict() for elem in PDFStructTree(self)]
        except StructTreeMissing:
            return []

    def to_dict(self, object_types: list[str] | None = None) -> dict[str, Any]:
        return {
            "metadata": self.metadata,
            "pages": [page.to_dict(object_types) for page in self.pages],
        }
