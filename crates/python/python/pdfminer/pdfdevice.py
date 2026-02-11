# pdfminer.pdfdevice compatibility shim

from bolivar._native_api import TagExtractor


class PDFDevice:
    def __init__(self, rsrcmgr: object | None = None) -> None:
        self.rsrcmgr = rsrcmgr

    def close(self) -> None:
        return None


__all__ = ["PDFDevice", "TagExtractor"]
