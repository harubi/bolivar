import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
PYTHON_SHIM = os.path.join(ROOT, "crates", "python", "python")
ROOT_PATH = Path(ROOT)
PYTHON_SHIM_PATH = Path(PYTHON_SHIM)


def _clear_modules():
    for name in list(sys.modules.keys()):
        if (
            name == "sitecustomize"
            or name.startswith("pdfplumber")
            or name.startswith("pdfminer")
        ):
            sys.modules.pop(name, None)


def _write_file(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def _make_shadow_package(root: Path, name: str, body: str) -> None:
    package_dir = root / name
    package_dir.mkdir(parents=True, exist_ok=True)
    _write_file(package_dir / "__init__.py", body)


def test_sitecustomize_autoload_patches_pdfplumber():
    _clear_modules()
    if PYTHON_SHIM not in sys.path:
        sys.path.insert(0, PYTHON_SHIM)

    import sitecustomize  # noqa: F401
    import pdfplumber

    assert (
        getattr(pdfplumber.page.Page.extract_tables, "_bolivar_patched", False) is True
    )


def test_sitecustomize_autoload_ignores_env_opt_out():
    env = os.environ.copy()
    env["BOLIVAR_AUTOLOAD"] = "0"
    code = (
        "import pdfplumber; "
        "print(getattr(pdfplumber.page.Page.extract_tables, '_bolivar_patched', False))"
    )
    result = subprocess.run(
        [sys.executable, "-c", code],
        check=True,
        capture_output=True,
        text=True,
        env=env,
    )
    lines = [line.strip() for line in result.stdout.splitlines() if line.strip()]
    assert lines, "expected subprocess output"
    assert lines[-1] == "True"


def test_autoload_prefers_bolivar_with_reference_path():
    env = os.environ.copy()
    env["ROOT"] = ROOT
    env["PYTHONPATH"] = f"{PYTHON_SHIM}:{ROOT}/references/pdfminer.six"
    code = (
        "import os, sys; "
        "sys.path.insert(0, os.path.join(os.environ['ROOT'], 'references', 'pdfminer.six')); "
        "import pdfminer, pdfplumber; "
        "print(pdfminer.__file__); "
        "print(getattr(pdfplumber.page.Page.extract_tables, '_bolivar_patched', False))"
    )
    result = subprocess.run(
        [sys.executable, "-c", code],
        check=True,
        capture_output=True,
        text=True,
        env=env,
    )
    lines = [line.strip() for line in result.stdout.splitlines() if line.strip()]
    assert lines, "expected subprocess output"
    assert "crates/python/python/pdfminer/__init__.py" in lines[0]
    assert lines[-1] == "True"


def test_sitecustomize_warns_on_failure():
    with tempfile.TemporaryDirectory() as temp_dir:
        shadow_dir = Path(temp_dir) / "shadow"
        shadow_dir.mkdir(parents=True, exist_ok=True)
        _write_file(
            shadow_dir / "bolivar_autoload.py",
            "raise ImportError('shadowed bolivar_autoload')\n",
        )
        env = os.environ.copy()
        env["PYTHONPATH"] = f"{shadow_dir}:{PYTHON_SHIM}"
        result = subprocess.run(
            [sys.executable, "-c", "print('ok')"],
            check=True,
            capture_output=True,
            text=True,
            env=env,
        )
        assert "bolivar autoload failed" in result.stderr.lower()


def test_autoload_pth_works_without_pythonpath():
    autoload_py = PYTHON_SHIM_PATH / "bolivar_autoload.py"
    assert autoload_py.exists(), "bolivar_autoload.py must exist"
    pth_path = PYTHON_SHIM_PATH / "bolivar_autoload.pth"
    pth_contents = pth_path.read_text(encoding="utf-8").strip()
    assert pth_contents.startswith("import bolivar_autoload"), (
        "pth must use bolivar_autoload"
    )
    with tempfile.TemporaryDirectory() as temp_dir:
        temp_path = Path(temp_dir)
        site_dir = temp_path / "site"
        site_dir.mkdir(parents=True, exist_ok=True)
        shutil.copy2(autoload_py, site_dir / "bolivar_autoload.py")
        shutil.copy2(pth_path, site_dir / "bolivar_autoload.pth")
        shutil.copytree(PYTHON_SHIM_PATH / "bolivar", site_dir / "bolivar")
        shutil.copytree(PYTHON_SHIM_PATH / "pdfminer", site_dir / "pdfminer")

        shadow_dir = temp_path / "shadow"
        _make_shadow_package(
            shadow_dir,
            "bolivar",
            "raise ImportError('shadowed bolivar')\n",
        )

        env = os.environ.copy()
        env["PYTHONPATH"] = str(shadow_dir)
        site_dir_arg = repr(str(site_dir))
        code = (
            "import site, sys; "
            f"site.addsitedir({site_dir_arg}); "
            "mod = sys.modules.get('pdfminer'); "
            "print(mod.__file__ if mod else 'none')"
        )
        result = subprocess.run(
            [sys.executable, "-S", "-c", code],
            check=True,
            capture_output=True,
            text=True,
            env=env,
        )
        lines = [line.strip() for line in result.stdout.splitlines() if line.strip()]
        assert lines, "expected subprocess output"
        assert str(site_dir / "pdfminer" / "__init__.py") in lines[0]
