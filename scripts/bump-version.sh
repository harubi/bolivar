#!/usr/bin/env bash
set -euo pipefail
VERSION="$1"

# All crates inherit via version.workspace = true
sed -i "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml

# bolivar-core workspace dep (path + version)
sed -i "s/\(bolivar-core = { path = \"crates\/core\", version = \)\".*\"/\1\"$VERSION\"/" Cargo.toml

# bolivar-icu workspace dep (path + version)
sed -i "s/\(bolivar-icu = { path = \"crates\/icu\", version = \)\".*\"/\1\"$VERSION\"/" Cargo.toml

# Regenerate Cargo.lock. Not `cargo check`: that runs build scripts, which
# made this job compile ICU from source on every release.
cargo update --workspace --quiet
