#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: $0 NATIVE_LIBRARY" >&2
    exit 2
fi

native_library=$1
case $(uname -s) in
    Darwin) native_dependencies=$(otool -L "$native_library") ;;
    Linux) native_dependencies=$(readelf -d "$native_library") ;;
    *)
        echo "unsupported dependency check platform: $(uname -s)" >&2
        exit 1
        ;;
esac

printf '%s\n' "$native_dependencies"
if printf '%s\n' "$native_dependencies" | grep -E 'libicu(uc|data|i18n)' >/dev/null; then
    echo "native library has a dynamic ICU dependency" >&2
    exit 1
fi
