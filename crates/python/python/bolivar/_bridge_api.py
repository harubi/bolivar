"""Internal native bridge for bolivar shim-only helpers."""

from __future__ import annotations

from importlib import import_module
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from types import ModuleType

    from bolivar._bolivar import (
        _extract_tables_for_page_indexed as _extract_tables_for_page_indexed,
    )
    from bolivar._bolivar import (
        _extract_tables_from_page_objects as _extract_tables_from_page_objects,
    )
    from bolivar._bolivar import (
        _extract_tables_stream as _extract_tables_stream,
    )
    from bolivar._bolivar import (
        _extract_text_stream as _extract_text_stream,
    )
    from bolivar._bolivar import (
        _extract_words_stream as _extract_words_stream,
    )

_NATIVE_MODULE: ModuleType | None = None


def load_bridge_api() -> ModuleType:
    """Load and memoize the native extension module for bridge-only helpers."""
    global _NATIVE_MODULE
    if _NATIVE_MODULE is None:
        _NATIVE_MODULE = import_module("bolivar._bolivar")
    return _NATIVE_MODULE


__all__ = [
    "_extract_tables_stream",
    "_extract_tables_for_page_indexed",
    "_extract_tables_from_page_objects",
    "_extract_text_stream",
    "_extract_words_stream",
]

_BRIDGE_EXPORTS = frozenset(__all__)


def __getattr__(name: str) -> object:
    if name not in _BRIDGE_EXPORTS:
        raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
    native = load_bridge_api()
    try:
        return getattr(native, name)
    except AttributeError as exc:
        raise AttributeError(
            f"native module bolivar._bolivar has no attribute {name!r}"
        ) from exc
