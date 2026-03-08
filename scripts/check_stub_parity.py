"""Verify every __all__ symbol in _native_api has a matching .pyi stub entry.

Exit 0 when every exported symbol is present in the stub and none are typed as
``Incomplete``.  Exit 1 otherwise, printing a summary of missing/incomplete
symbols.
"""

from __future__ import annotations

import ast
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NATIVE_API = ROOT / "crates" / "python" / "python" / "bolivar" / "_native_api.py"
EXPORT_MANIFEST = (
    ROOT / "crates" / "python" / "python" / "bolivar" / "_export_manifest.py"
)
STUB = ROOT / "crates" / "python" / "python" / "bolivar" / "_bolivar.pyi"


def _extract_string_sequence(node: ast.AST) -> list[str] | None:
    if not isinstance(node, (ast.List, ast.Tuple)):
        return None

    names: list[str] = []
    for elt in node.elts:
        if not isinstance(elt, ast.Constant) or not isinstance(elt.value, str):
            return None
        names.append(elt.value)
    return names


def _extract_manifest_exports(path: Path) -> dict[str, list[str]]:
    exports: dict[str, list[str]] = {}
    tree = ast.parse(path.read_text())
    for node in tree.body:
        if not isinstance(node, ast.Assign):
            continue
        names = _extract_string_sequence(node.value)
        if names is None:
            continue
        for target in node.targets:
            if isinstance(target, ast.Name):
                exports[target.id] = names
    return exports


def extract_all_names(path: Path, export_manifest: Path = EXPORT_MANIFEST) -> list[str]:
    """Parse ``__all__`` from a Python module."""
    manifest_exports = _extract_manifest_exports(export_manifest)
    tree = ast.parse(path.read_text())
    manifest_aliases: dict[str, str] = {}

    for node in tree.body:
        if isinstance(node, ast.ImportFrom) and node.module == "bolivar._export_manifest":
            for alias in node.names:
                manifest_aliases[alias.asname or alias.name] = alias.name
            continue

        if not isinstance(node, ast.Assign):
            continue
        for target in node.targets:
            if not isinstance(target, ast.Name) or target.id != "__all__":
                continue

            literal_names = _extract_string_sequence(node.value)
            if literal_names is not None:
                return literal_names

            if (
                isinstance(node.value, ast.Call)
                and isinstance(node.value.func, ast.Name)
                and node.value.func.id == "list"
                and len(node.value.args) == 1
                and not node.value.keywords
                and isinstance(node.value.args[0], ast.Name)
            ):
                manifest_name = manifest_aliases.get(node.value.args[0].id)
                if manifest_name is not None:
                    return manifest_exports.get(manifest_name, [])

            if isinstance(node.value, ast.Name):
                manifest_name = manifest_aliases.get(node.value.id)
                if manifest_name is not None:
                    return manifest_exports.get(manifest_name, [])

    return []


def extract_stub_symbols(path: Path) -> dict[str, str]:
    """Return a mapping of top-level symbol name → definition line from a .pyi stub."""
    symbols: dict[str, str] = {}
    top_level_def = re.compile(
        r"^(?:def|class|async def)\s+(\w+)|^(\w+)\s*[:=]",
    )
    for line in path.read_text().splitlines():
        m = top_level_def.match(line)
        if m:
            name = m.group(1) or m.group(2)
            if name:
                symbols[name] = line
    return symbols


def main() -> int:
    all_names = extract_all_names(NATIVE_API)
    if not all_names:
        print(f"ERROR: could not parse __all__ from {NATIVE_API}", file=sys.stderr)
        return 1

    stub_symbols = extract_stub_symbols(STUB)
    if not stub_symbols:
        print(f"ERROR: no symbols found in {STUB}", file=sys.stderr)
        return 1

    missing: list[str] = []
    incomplete: list[str] = []

    for name in all_names:
        if name not in stub_symbols:
            missing.append(name)
        elif "Incomplete" in stub_symbols[name]:
            incomplete.append(name)

    ok = True
    if missing:
        print(f"MISSING from stub ({len(missing)}):")
        for name in sorted(missing):
            print(f"  - {name}")
        ok = False
    if incomplete:
        print(f"INCOMPLETE in stub ({len(incomplete)}):")
        for name in sorted(incomplete):
            print(f"  - {name}")
        ok = False

    if ok:
        print(f"OK: all {len(all_names)} symbols present in stub")
        return 0
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
