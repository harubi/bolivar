#!/usr/bin/env bash
set -euo pipefail
VERSION="$1"

# All crates inherit via version.workspace = true
sed -i "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml

# bolivar-core workspace dep (path + version)
sed -i "s/\(bolivar-core = { path = \"crates\/core\", version = \)\".*\"/\1\"$VERSION\"/" Cargo.toml

sed -i "s/^version = \".*\"/version = \"$VERSION\"/" crates/uniffi/jvm/build.gradle.kts
