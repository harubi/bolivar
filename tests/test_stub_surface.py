from __future__ import annotations

import re
from pathlib import Path

from bolivar._export_manifest import BRIDGE_EXPORTS, PUBLIC_EXPORTS

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


def test_removed_legacy_surface_is_absent_from_stubs() -> None:
    symbols = _load_stub_symbols()
    assert "async_runtime_poc" not in symbols
    assert "extract_pages_async_from_document" not in symbols
    assert "PyTableStream" not in symbols


def test_native_stub_symbols_match_manifest() -> None:
    from bolivar._export_manifest import NATIVE_EXTENSION_EXPORTS, STUB_SUPPORT_SYMBOLS

    symbols = _load_stub_symbols()
    expected = (
        set(PUBLIC_EXPORTS)
        | set(BRIDGE_EXPORTS)
        | set(NATIVE_EXTENSION_EXPORTS)
        | set(STUB_SUPPORT_SYMBOLS)
    )
    assert symbols == expected


def test_compat_table_helper_stub_uses_explicit_object_lists() -> None:
    stub_text = _STUB_PATH.read_text()
    match = re.search(
        r"def _extract_tables_for_compat_page\((?P<body>.*?)\) ->",
        stub_text,
        re.S,
    )
    assert match is not None
    body = match.group("body")
    assert "chars:" in body
    assert "lines:" in body
    assert "rects:" in body
    assert "curves:" in body
    assert "geometry:" in body
    assert "table_settings:" in body
    assert "objects:" not in body
