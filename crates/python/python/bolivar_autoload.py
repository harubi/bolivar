import contextlib
import os
import sys


_BOLIVAR_CANONICAL_AUTOLOAD = True


def _warn(exc: Exception) -> None:
    with contextlib.suppress(Exception):
        sys.stderr.write(f"bolivar autoload failed: {exc}\n")


def _ensure_sys_path(base: str) -> None:
    if sys.path and sys.path[0] == base:
        return
    with contextlib.suppress(ValueError):
        sys.path.remove(base)
    sys.path.insert(0, base)


def install() -> bool:
    try:
        base = os.path.abspath(os.path.dirname(__file__))
        _ensure_sys_path(base)
        from bolivar import _autoload

        return bool(_autoload.install())
    except Exception as exc:
        _warn(exc)
        return False
