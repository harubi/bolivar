from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib


def test_maturin_profile_is_release():
    data = tomllib.loads(Path("pyproject.toml").read_text())
    profile = data.get("tool", {}).get("maturin", {}).get("profile")
    assert profile == "release"
