import contextlib
import importlib.util
import os
import sys
from types import ModuleType


def _warn(exc: Exception) -> None:
    with contextlib.suppress(Exception):
        sys.stderr.write(f"bolivar autoload failed: {exc}\n")


def _ensure_sys_path(base: str) -> None:
    if sys.path and sys.path[0] == base:
        return
    with contextlib.suppress(ValueError):
        sys.path.remove(base)
    sys.path.insert(0, base)


def _load_registry_module(base: str) -> ModuleType:
    path = os.path.join(base, "bolivar", "_shim_registry.py")
    spec = importlib.util.spec_from_file_location("bolivar._shim_registry", path)
    if spec is None or spec.loader is None:
        raise ImportError("bolivar._shim_registry not found")
    module = importlib.util.module_from_spec(spec)
    sys.modules.setdefault("bolivar._shim_registry", module)
    spec.loader.exec_module(module)
    return module


def install() -> bool:
    try:
        base = os.path.abspath(os.path.dirname(__file__))
        _ensure_sys_path(base)
        module = _load_registry_module(base)
        module.install(base=base)
        return True
    except Exception as exc:
        _warn(exc)
        return False
