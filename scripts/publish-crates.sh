#!/usr/bin/env bash
set -euo pipefail

echo "Publishing bolivar-core to crates.io..."
cargo publish -p bolivar-core --allow-dirty --locked

echo "Publishing bolivar-cli to crates.io..."
cargo publish -p bolivar-cli --allow-dirty --locked
