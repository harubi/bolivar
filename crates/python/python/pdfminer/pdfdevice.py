# pdfminer.pdfdevice compatibility shim

from bolivar._bolivar import TagExtractor


class PDFDevice:
    def __init__(self, rsrcmgr=None):
        self.rsrcmgr = rsrcmgr

    def close(self):
        return None


__all__ = ["PDFDevice", "TagExtractor"]
