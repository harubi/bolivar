use std::env;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=src/layout/icu_bidi.c");
    println!("cargo:rerun-if-env-changed=ICU_ROOT");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
    println!("cargo:rerun-if-env-changed=BOLIVAR_DYNAMIC_ICU");

    let target = env::var("TARGET").expect("Cargo did not set TARGET");
    let include_paths = if target.contains("windows-msvc") {
        vcpkg::Config::new()
            .find_package("icu")
            .expect("ICU4C is required to build bolivar-core")
            .include_paths
    } else {
        configure_macos_pkg_config(&target);
        let link_static = env::var("BOLIVAR_DYNAMIC_ICU").as_deref() != Ok("1");
        pkg_config::Config::new()
            .statik(link_static)
            .probe("icu-uc")
            .expect("ICU4C is required to build bolivar-core")
            .include_paths
    };

    let mut build = cc::Build::new();
    build.file("src/layout/icu_bidi.c");
    for path in include_paths {
        build.include(path);
    }
    build.compile("bolivar_icu_bidi");
}

fn configure_macos_pkg_config(target: &str) {
    if !target.contains("apple-darwin") || env::var_os("PKG_CONFIG_PATH").is_some() {
        return;
    }

    if let Some(root) = env::var_os("ICU_ROOT") {
        set_pkg_config_path(Path::new(&root).join("lib/pkgconfig"));
        return;
    }

    for root in [
        "/opt/homebrew/opt/icu4c@78",
        "/opt/homebrew/opt/icu4c",
        "/usr/local/opt/icu4c@78",
        "/usr/local/opt/icu4c",
    ] {
        let path = Path::new(root).join("lib/pkgconfig");
        if path.is_dir() {
            set_pkg_config_path(path);
            return;
        }
    }
}

fn set_pkg_config_path(path: impl AsRef<Path>) {
    // The build script is single-threaded and sets this before pkg-config runs.
    unsafe { env::set_var("PKG_CONFIG_PATH", path.as_ref()) };
}
