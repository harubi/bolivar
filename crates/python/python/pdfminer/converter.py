# pdfminer.converter compatibility shim
#
# PDFPageAggregator is kept in pure Python for subclassability.
# pdfplumber subclasses it as PDFPageAggregatorWithMarkedContent.


class PDFPageAggregator:
    """Collects layout items from a PDF page.

    Pure Python implementation for subclassability.
    """

    def __init__(self, rsrcmgr, pageno=1, laparams=None):
        self.rsrcmgr = rsrcmgr
        self.pageno = pageno
        self.laparams = laparams
        self.cur_item = None

    def begin_page(self, page, ctm):
        """Called at the start of page processing."""
        pass

    def end_page(self, page):
        """Called at the end of page processing."""
        pass

    def begin_figure(self, name, bbox, matrix):
        """Called when entering a figure."""
        pass

    def end_figure(self, name):
        """Called when exiting a figure."""
        pass

    def receive_layout(self, ltpage):
        """Receive the analyzed layout for a page."""
        self.cur_item = ltpage

    def get_result(self):
        """Get the current page's layout result."""
        return self.cur_item


class PDFConverter:
    """Base class for PDF converters - stub."""
    pass


class TextConverter:
    """Converts PDF to plain text - stub."""

    def __init__(self, rsrcmgr, outfp, codec="utf-8", pageno=1, laparams=None,
                 showpageno=False, imagewriter=None):
        raise NotImplementedError("TextConverter not yet implemented via bolivar")


class HTMLConverter:
    """Converts PDF to HTML - stub."""

    def __init__(self, rsrcmgr, outfp, codec="utf-8", pageno=1, laparams=None,
                 scale=1, fontscale=1.0, layoutmode="normal", showpageno=True,
                 imagewriter=None):
        raise NotImplementedError("HTMLConverter not yet implemented via bolivar")


class XMLConverter:
    """Converts PDF to XML - stub."""

    def __init__(self, rsrcmgr, outfp, codec="utf-8", pageno=1, laparams=None,
                 stripcontrol=False, imagewriter=None):
        raise NotImplementedError("XMLConverter not yet implemented via bolivar")
