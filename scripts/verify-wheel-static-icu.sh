#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: $0 WHEEL_DIRECTORY" >&2
    exit 2
fi

wheel_directory=$1

wheel_count=0
for wheel_path in "$wheel_directory"/*.whl; do
    [ -e "$wheel_path" ] || continue
    wheel_count=$((wheel_count + 1))

    wheel_extract_dir=$(mktemp -d)
    unzip -q "$wheel_path" -d "$wheel_extract_dir"

    library_count=0
    for native_library in $(find "$wheel_extract_dir" -type f \( -name '*.so' -o -name '*.dylib' \) -print); do
        library_count=$((library_count + 1))
        echo "Checking $(basename "$wheel_path") -> $(basename "$native_library")"
        sh scripts/verify-static-icu.sh "$native_library"
    done

    rm -rf -- "$wheel_extract_dir"

    if [ "$library_count" -eq 0 ]; then
        echo "native library not found in $wheel_path" >&2
        exit 1
    fi
done

if [ "$wheel_count" -eq 0 ]; then
    echo "wheel not found in $wheel_directory" >&2
    exit 1
fi

echo "Verified static ICU in $wheel_count wheel(s)"
