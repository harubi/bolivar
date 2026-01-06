import importlib.util
import os
import sys


def _shim_paths():
    base = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
    shim_dir = os.path.join(base, "pdfminer")
    shim_init = os.path.join(shim_dir, "__init__.py")
    return shim_dir, shim_init


def _load_shim():
    shim_dir, shim_init = _shim_paths()
    spec = importlib.util.spec_from_file_location(
        "pdfminer", shim_init, submodule_search_locations=[shim_dir]
    )
    if spec is None or spec.loader is None:
        raise ImportError("bolivar pdfminer shim not found")
    module = importlib.util.module_from_spec(spec)
    module.__path__ = [shim_dir]
    sys.modules["pdfminer"] = module
    spec.loader.exec_module(module)
    return module


def install():
    for name in list(sys.modules.keys()):
        if name == "pdfminer" or name.startswith("pdfminer."):
            sys.modules.pop(name, None)

    module = _load_shim()
    try:
        from pdfminer._bolivar_patch import patch_pdfplumber
    except Exception:
        patch_pdfplumber = None
    if patch_pdfplumber is not None:
        patch_pdfplumber()
    return module
