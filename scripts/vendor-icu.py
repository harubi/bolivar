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

VERSION = "79.1"
URL = (
    "https://github.com/unicode-org/icu/releases/download/"
    f"release-{VERSION}/icu4c-{VERSION}-sources.tgz"
)
SHA256 = "06fa63ecb521048f8082f12ad988f6b678d90cbd88aafce24cbc94902b191d99"

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

# ICU 79 added MessageFormat 2 helpers (ubidi_getMFOption, ubidi_isolate) to
# ubidi.cpp behind U_SHOW_CPLUSPLUS_API only. They use UnicodeString, which
# pulls in the C++ runtime, mutexes and platform code. Only i18n's
# messageformat2.cpp calls them, so also guard them with UCONFIG_NO_MF2;
# build.rs defines UCONFIG_NO_MF2=1. Each patch must apply exactly once.
PATCHES = {
    "ubidi.cpp": [
        (
            b"#if U_SHOW_CPLUSPLUS_API\n",
            b"#if U_SHOW_CPLUSPLUS_API && !UCONFIG_NO_MF2\n",
        ),
    ],
}

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

    for name, patches in PATCHES.items():
        for old, new in patches:
            count = common[name].count(old)
            if count != 1:
                sys.exit(f"patch for {name} matched {count} times; review it")
            common[name] = common[name].replace(old, new)

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
