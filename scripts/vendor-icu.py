#!/usr/bin/env python3
"""Vendor the ICU4C sources that the inverse bidi bridge needs.

bolivar-icu calls only ubidi. ICU compiles the bidi property tables into its
sources, so these files need no ICU data library. The set was found by
linking the bridge against an archive of all of icu4c/source/common and
reading which objects the linker pulled in.

Usage: python3 scripts/vendor-icu.py
Then run `cargo test -p bolivar-icu`; the golden test pins the output.
"""

from __future__ import annotations

import hashlib
import io
import re
import shutil
import sys
import tarfile
import urllib.request
from pathlib import Path

VERSION = "78.3"
URL = (
    "https://github.com/unicode-org/icu/releases/download/"
    f"release-{VERSION}/icu4c-{VERSION}-sources.tgz"
)
SHA256 = "3a2e7a47604ba702f345878308e6fefeca612ee895cf4a5f222e7955fabfe0c0"

SOURCES = [
    "cmemory.cpp",
    "cstring.cpp",
    "ubidi.cpp",
    "ubidi_props.cpp",
    "ubidiln.cpp",
    "ubidiwrt.cpp",
    "uchar.cpp",
    "ucptrie.cpp",
    "udataswp.cpp",
    "uinvchar.cpp",
    "ustring.cpp",
    "utf_impl.cpp",
    "utrie2.cpp",
]
# Headers the bridge includes directly.
ROOT_HEADERS = ["unicode/ubidi.h", "unicode/utypes.h"]

REPO = Path(__file__).resolve().parent.parent
DESTINATION = REPO / "crates/icu/icu4c"
INCLUDE = re.compile(rb'^\s*#\s*include\s*[<"]([^>"]+)[>"]', re.MULTILINE)


def fetch() -> bytes:
    with urllib.request.urlopen(URL) as response:
        archive = response.read()
    digest = hashlib.sha256(archive).hexdigest()
    if digest != SHA256:
        sys.exit(f"checksum mismatch for {URL}: {digest}")
    return archive


def main() -> None:
    with tarfile.open(fileobj=io.BytesIO(fetch()), mode="r:gz") as archive:
        prefix = "icu/source/common/"
        common = {
            member.name.removeprefix(prefix): archive.extractfile(member).read()
            for member in archive.getmembers()
            if member.isfile() and member.name.startswith(prefix)
        }
        license_text = archive.extractfile("icu/LICENSE").read()

    # Follow every #include, including ones behind platform #ifs, so the
    # set is complete for every target and not only for this host.
    wanted: set[str] = set()
    pending = SOURCES + ROOT_HEADERS
    while pending:
        name = pending.pop()
        if name in wanted:
            continue
        wanted.add(name)
        directory = str(Path(name).parent)
        for include in INCLUDE.findall(common[name]):
            include = include.decode()
            for candidate in (include, str(Path(directory, include))):
                candidate = str(Path(candidate))
                if candidate in common:
                    pending.append(candidate)
                    break

    shutil.rmtree(DESTINATION, ignore_errors=True)
    for name in sorted(wanted):
        path = DESTINATION / "common" / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(common[name])
    (REPO / "crates/icu/LICENSE-ICU").write_bytes(license_text)
    (DESTINATION / "VERSION").write_text(f"{VERSION}\n")
    print(f"vendored {len(wanted)} files from ICU4C {VERSION} into {DESTINATION}")


if __name__ == "__main__":
    main()
