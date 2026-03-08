import contextlib
import sys


def _warn(exc: Exception) -> None:
    with contextlib.suppress(Exception):
        sys.stderr.write(f"bolivar autoload failed: {exc}\n")


def _install_preloaded_top_level_autoload() -> bool | None:
    module = sys.modules.get("bolivar_autoload")
    if module is None or getattr(module, "_BOLIVAR_CANONICAL_AUTOLOAD", False):
        return None
    install = getattr(module, "install", None)
    if install is None:
        return None
    return bool(install())


def install() -> bool:
    top_level = _install_preloaded_top_level_autoload()
    if top_level is not None:
        return top_level
    try:
        from . import _shim_registry as shim_registry

        shim_registry.install()
        return True
    except Exception as exc:
        _warn(exc)
        return False
