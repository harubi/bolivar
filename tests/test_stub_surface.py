from __future__ import annotations

import re
from pathlib import Path

_TOP_LEVEL_SYMBOL = re.compile(r"^(?:def|class|async def)\s+(\w+)|^(\w+)\s*[:=]")
_STUB_PATH = (
    Path(__file__).resolve().parents[1]
    / "crates"
    / "python"
    / "python"
    / "bolivar"
    / "_bolivar.pyi"
)


def _load_stub_symbols() -> set[str]:
    symbols: set[str] = set()
    for line in _STUB_PATH.read_text().splitlines():
        match = _TOP_LEVEL_SYMBOL.match(line)
        if match is None:
            continue
        name = match.group(1) or match.group(2)
        if name:
            symbols.add(name)
    return symbols


def test_stream_exports_are_present_in_native_stub() -> None:
    symbols = _load_stub_symbols()
    assert "extract_pages_async_from_document" in symbols
    assert "PyTableStream" in symbols
