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


def test_layout_objects_are_canonical():
    page = next(iter(extract_pages(SAMPLE)))
    chars = [obj for obj in _walk(page) if obj.__class__.__name__ == "LTChar"]
    assert chars
    assert chars[0].__class__.__module__.startswith("bolivar")

    from pdfminer.layout import LTLine

    line = LTLine(
        1.0, [(0.0, 0.0), (1.0, 1.0)], True, False, False, (0.0,), (0.0,), None
    )
    assert line.__class__.__module__.startswith("bolivar")
