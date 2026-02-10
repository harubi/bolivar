#!/usr/bin/env bash
set -euo pipefail

REGISTRY_API_BASE="${REGISTRY_API_BASE:-https://crates.io/api/v1}"
CRATES=("bolivar-core" "bolivar-cli")
MISSING=()

check_crate_exists() {
  local crate="$1"
  local status

  status="$(
    curl --silent --show-error --location \
      --output /dev/null \
      --write-out '%{http_code}' \
      "${REGISTRY_API_BASE}/crates/${crate}"
  )"

  case "$status" in
    200)
      echo "Found crate on crates.io: ${crate}"
      ;;
    404)
      MISSING+=("$crate")
      ;;
    *)
      echo "Failed to verify crate '${crate}' (HTTP ${status}) via ${REGISTRY_API_BASE}" >&2
      exit 1
      ;;
  esac
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

