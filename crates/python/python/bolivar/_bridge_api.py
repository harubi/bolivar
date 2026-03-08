"""Internal native bridge for bolivar shim-only helpers."""

from __future__ import annotations

from collections.abc import Callable
from importlib import import_module
from typing import TYPE_CHECKING

from bolivar._export_manifest import BRIDGE_EXPORTS

if TYPE_CHECKING:
    from types import ModuleType

    from bolivar._bolivar import (
        _extract_tables_for_page_indexed as _extract_tables_for_page_indexed,
    )
    from bolivar._bolivar import (
        _extract_tables_for_compat_page as _extract_tables_for_compat_page,
    )
    from bolivar._bolivar import (
        _extract_words_for_page_indexed as _extract_words_for_page_indexed,
    )

_NATIVE_MODULE: ModuleType | None = None
_BRIDGE_EXPORT_NAMES = frozenset(BRIDGE_EXPORTS)
_MODULE_DUNDER_NAMES = frozenset(
    name for name in globals() if name.startswith("__")
)


def _load_bridge_api() -> ModuleType:
    """Load and memoize the native extension module for bridge-only helpers."""
    global _NATIVE_MODULE
    if _NATIVE_MODULE is None:
        _NATIVE_MODULE = import_module("bolivar._bolivar")
    return _NATIVE_MODULE


__all__ = list(BRIDGE_EXPORTS)


def __getattr__(
    name: str,
    _load_bridge_api: Callable[[], ModuleType] = _load_bridge_api,
) -> object:
    if name not in _BRIDGE_EXPORT_NAMES:
        raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
    native = _load_bridge_api()
    try:
        return getattr(native, name)
    except AttributeError as exc:
        raise AttributeError(
            f"native module bolivar._bolivar has no attribute {name!r}"
        ) from exc


def __dir__() -> list[str]:
    return sorted(_MODULE_DUNDER_NAMES | _BRIDGE_EXPORT_NAMES)


del _load_bridge_api
