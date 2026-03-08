import contextlib
import sys


def _warn(exc: Exception) -> None:
    with contextlib.suppress(Exception):
        sys.stderr.write(f"bolivar autoload failed: {exc}\n")


def install() -> bool:
    try:
        from . import _shim_registry as shim_registry

        shim_registry.install()
        return True
    except Exception as exc:
        _warn(exc)
        return False
