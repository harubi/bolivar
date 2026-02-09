#!/usr/bin/env bash
set -euo pipefail

cd crates/uniffi/jvm
./gradlew publishAndReleaseToMavenCentral
