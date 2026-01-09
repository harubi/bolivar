from pathlib import Path
import tomllib


def test_maturin_profile_is_release():
    data = tomllib.loads(Path("pyproject.toml").read_text())
    profile = data.get("tool", {}).get("maturin", {}).get("profile")
    assert profile == "release"
