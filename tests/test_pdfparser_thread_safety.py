import gc
import queue
import sys
import threading
from io import BytesIO

from pdfminer.pdfparser import PDFParser


def test_pdfparser_drop_on_different_thread_has_no_unsendable_runtimeerror():
    unraisable: list[BaseException] = []
    old_unraisablehook = sys.unraisablehook

    def capture_unraisable(args: object) -> None:
        exc_value = getattr(args, "exc_value", None)
        if isinstance(exc_value, BaseException):
            unraisable.append(exc_value)

    sys.unraisablehook = capture_unraisable
    try:
        parser_queue: queue.Queue[PDFParser] = queue.Queue()

        def create_parser_on_worker() -> None:
            parser_queue.put(PDFParser(BytesIO(b"1 0 R")))

        worker = threading.Thread(target=create_parser_on_worker)
        worker.start()
        worker.join()

        parser = parser_queue.get(timeout=1.0)
        del parser
        gc.collect()
    finally:
        sys.unraisablehook = old_unraisablehook

    assert not any(
        isinstance(exc, RuntimeError)
        and "is unsendable, but is being dropped on another thread" in str(exc)
        for exc in unraisable
    ), f"Unexpected unsendable drop errors captured: {unraisable!r}"
