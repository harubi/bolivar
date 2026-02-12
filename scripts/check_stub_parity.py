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
STUB = ROOT / "crates" / "python" / "python" / "bolivar" / "_bolivar.pyi"


def extract_all_names(path: Path) -> list[str]:
    """Parse ``__all__`` from a Python module."""
    tree = ast.parse(path.read_text())
    for node in ast.walk(tree):
        if isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name) and target.id == "__all__":
                    if isinstance(node.value, ast.List):
                        return [
                            elt.value
                            for elt in node.value.elts
                            if isinstance(elt, ast.Constant) and isinstance(elt.value, str)
                        ]
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
