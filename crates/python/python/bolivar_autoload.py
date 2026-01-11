import importlib.util
import os
import sys


def _warn(exc: Exception) -> None:
    try:
        sys.stderr.write(f"bolivar autoload failed: {exc}\n")
    except Exception:
        pass


def _ensure_sys_path(base: str) -> None:
    if sys.path and sys.path[0] == base:
        return
    try:
        sys.path.remove(base)
    except ValueError:
        pass
    sys.path.insert(0, base)


def _load_autoload_module(base: str):
    path = os.path.join(base, "bolivar", "_autoload.py")
    spec = importlib.util.spec_from_file_location("bolivar._autoload", path)
    if spec is None or spec.loader is None:
        raise ImportError("bolivar._autoload not found")
    module = importlib.util.module_from_spec(spec)
    sys.modules.setdefault("bolivar._autoload", module)
    spec.loader.exec_module(module)
    return module


def install() -> bool:
    try:
        base = os.path.abspath(os.path.dirname(__file__))
        _ensure_sys_path(base)
        module = _load_autoload_module(base)
        module.install()
        return True
    except Exception as exc:
        _warn(exc)
        return False
