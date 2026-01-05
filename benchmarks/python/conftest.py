import gc
import json
from pathlib import Path
import os

import pytest

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "benchmarks" / "fixtures.json"


def _load_manifest():
    data = json.loads(MANIFEST.read_text())
    assert data.get("version") == 1
    return data


def _load_fixtures(tag=None, tier=None):
    data = _load_manifest()
    fixtures = []
    for fx in data["fixtures"]:
        if tier and tier not in fx.get("tiers", []):
            continue
        if tag and tag not in fx.get("tags", []):
            continue
        fixtures.append(fx)
    return fixtures


def pytest_generate_tests(metafunc):
    if "text_fixture" in metafunc.fixturenames:
        tier = "full" if os.environ.get("BOLIVAR_BENCH_TIER") == "full" else "quick"
        fixtures = _load_fixtures(tag="text", tier=tier)
        params = []
        ids = []
        for fx in fixtures:
            path = ROOT / fx["path"]
            params.append((fx, path.read_bytes()))
            ids.append(fx["id"])
        metafunc.parametrize("text_fixture", params, ids=ids)


@pytest.fixture(autouse=True)
def disable_gc():
    gc.disable()
    yield
    gc.enable()
