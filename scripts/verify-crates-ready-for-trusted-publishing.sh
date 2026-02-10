#!/usr/bin/env bash
set -euo pipefail

CRATES=("bolivar-core" "bolivar-cli")
MISSING=()

check_crate_exists() {
  local crate="$1"
  local output

  # Use cargo's official crates.io client path instead of direct HTTP probing.
  if output="$(cargo owner --list "${crate}" 2>&1)"; then
    echo "Found crate on crates.io: ${crate}"
    return
  fi

  if grep -qE "status 404 Not Found|does not exist" <<<"${output}"; then
      echo "Crate not found on crates.io: ${crate}"
      MISSING+=("$crate")
    return
  fi

  echo "Failed to verify crate '${crate}' via cargo owner --list" >&2
  echo "${output}" >&2
  exit 1
}

for crate in "${CRATES[@]}"; do
  check_crate_exists "$crate"
done

if ((${#MISSING[@]} > 0)); then
  echo "Trusted Publishing cannot create new crates yet." >&2
  echo "Manual bootstrap publish required for: ${MISSING[*]}" >&2
  echo "Publish each missing crate manually once (e.g. with a crates.io API token), then re-run release." >&2
  exit 1
fi
