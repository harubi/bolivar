from pdfminer.pdftypes import PDFObjRef, resolve1


class DummyDoc:
    def __init__(self):
        self.calls = 0

    def getobj(self, objid):
        self.calls += 1
        return f"obj{objid}"


def test_resolve1_calls_getobj():
    doc = DummyDoc()
    obj = PDFObjRef(doc, 1)
    assert resolve1(obj) == "obj1"
    assert doc.calls == 1
