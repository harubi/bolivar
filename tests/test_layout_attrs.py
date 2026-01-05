from pdfminer.high_level import extract_pages

SAMPLE = "references/pdfminer.six/samples/simple1.pdf"


def _walk(items):
    for obj in items:
        yield obj
        if hasattr(obj, "__iter__"):
            try:
                yield from _walk(list(obj))
            except TypeError:
                continue


def test_layout_objects_have_expected_attrs():
    page = next(iter(extract_pages(SAMPLE)))
    chars = [obj for obj in _walk(page) if obj.__class__.__name__ == "LTChar"]
    assert chars
    c = chars[0]
    assert hasattr(c, "graphicstate")
    assert hasattr(c, "ncs")
    assert hasattr(c, "scs")
    assert hasattr(c, "adv")
