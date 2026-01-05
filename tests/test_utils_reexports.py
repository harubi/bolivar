import bolivar._bolivar as core
import pdfminer.data_structures as ds
import pdfminer.utils as u


def test_utils_reexported_from_rust():
    assert u.decode_text is core.decode_text
    assert u.isnumber is core.isnumber
    assert u.PDFDocEncoding == core.PDFDocEncoding
    assert u.INF == core.INF
    assert u.MATRIX_IDENTITY == core.MATRIX_IDENTITY


def test_numbertree_reexported_from_rust():
    assert ds.NumberTree is core.NumberTree
