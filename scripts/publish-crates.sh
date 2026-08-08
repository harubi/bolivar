#!/usr/bin/env bash
set -euo pipefail

# Skips versions already on crates.io, so a partial failure can be retried.
publish() {
  local crate="$1" log
  echo "Publishing ${crate} to crates.io..."
  if log="$(cargo publish -p "${crate}" --locked 2>&1)"; then
    echo "${log}"
    return 0
  fi
  echo "${log}"
  if grep -qiE "already (been )?(uploaded|exists)|is already uploaded" <<<"${log}"; then
    echo "Skipping ${crate}: this version is already on crates.io"
    return 0
  fi
  return 1
}

# Dependency order.
publish bolivar-icu
publish bolivar-core
publish bolivar-cli
