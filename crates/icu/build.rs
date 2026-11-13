//! Compiles the inverse bidi bridge against a vendored ICU4C subset.
//!
//! `icu4c/common` holds only the ICU sources that `ubidi` needs; see
//! `scripts/vendor-icu.py`. ICU compiles the bidi property tables into these
//! sources, so no ICU data library is linked. The subset uses only the C
//! library, so no C++ standard library is linked either.

const SOURCES: &[&str] = &[
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
];

fn main() {
    println!("cargo:rerun-if-changed=src/icu_bidi.c");
    println!("cargo:rerun-if-changed=icu4c");

    // The bridge goes first: Cargo passes archives to the linker in this
    // order, and GNU ld only resolves a symbol from a later archive.
    cc::Build::new()
        .file("src/icu_bidi.c")
        .include("icu4c/common")
        .flag_if_supported("-fvisibility=hidden")
        .compile("bolivar_icu_bidi");

    let mut icu = cc::Build::new();
    icu.cpp(true)
        .std("c++17")
        .cpp_link_stdlib(None)
        .include("icu4c/common")
        .define("U_COMMON_IMPLEMENTATION", None)
        .define("U_STATIC_IMPLEMENTATION", None)
        // ICU's own Windows build compiles its sources as UTF-8.
        .flag_if_supported("/utf-8")
        .flag_if_supported("-fvisibility=hidden")
        .warnings(false);
    for source in SOURCES {
        icu.file(format!("icu4c/common/{source}"));
    }
    icu.compile("bolivar_icu_common");
}
