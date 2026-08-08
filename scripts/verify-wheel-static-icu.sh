#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: $0 WHEEL_DIRECTORY" >&2
    exit 2
fi

wheel_directory=$1
wheel_path=$(find "$wheel_directory" -maxdepth 1 -name '*.whl' -print | head -n 1)
if [ -z "$wheel_path" ]; then
    echo "wheel not found in $wheel_directory" >&2
    exit 1
fi

wheel_extract_dir=$(mktemp -d)
trap 'rm -rf -- "$wheel_extract_dir"' EXIT HUP INT TERM
unzip -q "$wheel_path" -d "$wheel_extract_dir"
native_library=$(find "$wheel_extract_dir" -type f \( -name '*.so' -o -name '*.dylib' \) -print | head -n 1)
if [ -z "$native_library" ]; then
    echo "native library not found in $wheel_path" >&2
    exit 1
fi

sh scripts/verify-static-icu.sh "$native_library"
