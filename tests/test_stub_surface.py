from __future__ import annotations

import re
from pathlib import Path

from bolivar._export_manifest import STUB_SYMBOLS
from bolivar._export_manifest import PUBLIC_EXPORTS
from scripts import check_stub_parity

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
    symbols = _load_stub_symbols()
    assert symbols == set(STUB_SYMBOLS)


def test_stub_parity_parser_reads_manifest_driven_native_all() -> None:
    assert check_stub_parity.extract_all_names(check_stub_parity.NATIVE_API) == list(
        PUBLIC_EXPORTS
    )


def test_stub_manifest_declares_stub_symbols_once() -> None:
    manifest_path = _STUB_PATH.with_name("_export_manifest.py")
    manifest_text = manifest_path.read_text()

    assert manifest_text.count("STUB_SYMBOLS =") == 1


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
