#!/usr/bin/env bash
set -euo pipefail

cd crates/ffi/jvm
./gradlew publishAndReleaseToMavenCentral
