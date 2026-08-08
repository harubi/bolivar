use std::env;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};

const ICU_VERSION: &str = "78.3";
const ICU_ARCHIVE_SHA256: &str = "3a2e7a47604ba702f345878308e6fefeca612ee895cf4a5f222e7955fabfe0c0";
const ICU_ARCHIVE_URL: &str =
    "https://github.com/unicode-org/icu/releases/download/release-78.3/icu4c-78.3-sources.tgz";

fn main() {
    println!("cargo:rerun-if-changed=src/icu_bidi.c");
    println!("cargo:rerun-if-env-changed=DOCS_RS");
    println!("cargo:rerun-if-env-changed=VCPKG_ROOT");
    println!("cargo:rerun-if-env-changed=VCPKGRS_DYNAMIC");
    println!("cargo:rerun-if-env-changed=VCPKGRS_TRIPLET");

    if env::var_os("DOCS_RS").is_some() {
        return;
    }

    let target = env::var("TARGET").expect("Cargo did not set TARGET");
    if target.contains("windows-msvc") {
        let library = vcpkg::Config::new()
            .find_package("icu")
            .expect("static ICU4C is required for the MSVC target");
        assert!(library.is_static, "ICU4C must be linked statically");
        compile_bridge(library.include_paths);
    } else if target.contains("apple-darwin") || target.contains("linux") {
        let prefix = build_unix_icu(&target).expect("failed to build static ICU4C");
        compile_bridge([prefix.join("include")]);
        link_unix_icu(&prefix);
    } else {
        panic!("unsupported ICU target: {target}");
    }
}

fn compile_bridge(include_paths: impl IntoIterator<Item = PathBuf>) {
    let mut build = cc::Build::new();
    build.file("src/icu_bidi.c");
    for path in include_paths {
        build.include(path);
    }
    build.compile("bolivar_icu_bidi");
}

fn build_unix_icu(target: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let host = env::var("HOST")?;
    if host != target {
        return Err(format!(
            "ICU source builds require a native target host: host={host} target={target}"
        )
        .into());
    }

    let output_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("Cargo did not set OUT_DIR")?);
    let cache_dir = cache_dir(&output_dir, target)?;
    let prefix = cache_dir.join("install");
    if !prefix.join("lib/libicuuc.a").is_file() || !prefix.join("lib/libicudata.a").is_file() {
        download_and_build(&cache_dir, &prefix, target)?;
    }

    Ok(prefix)
}

fn link_unix_icu(prefix: &Path) {
    println!(
        "cargo:rustc-link-search=native={}",
        prefix.join("lib").display()
    );
    println!("cargo:rustc-link-lib=static=icuuc");
    println!("cargo:rustc-link-lib=static=icudata");
    println!("cargo:rustc-link-lib=pthread");
    println!("cargo:rustc-link-lib=m");
}

fn cache_dir(output_dir: &Path, target: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let profile = env::var("PROFILE")?;
    let profile_dir = output_dir
        .ancestors()
        .find(|path| path.file_name() == Some(OsStr::new(&profile)))
        .ok_or("could not find the Cargo profile directory")?;
    let target_dir = profile_dir
        .parent()
        .ok_or("could not find the Cargo target directory")?;
    let cache_dir = target_dir
        .join("bolivar-icu")
        .join(target)
        .join(ICU_VERSION);
    fs::create_dir_all(&cache_dir)?;
    Ok(cache_dir)
}

fn download_and_build(
    output_dir: &Path,
    prefix: &Path,
    target: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let archive_path = output_dir.join(format!("icu4c-{ICU_VERSION}.tgz"));
    ensure_archive(&archive_path)?;

    let source_root = output_dir.join(format!("icu4c-{ICU_VERSION}-source"));
    let build_root = output_dir.join(format!("icu4c-{ICU_VERSION}-build"));
    if !source_root.join("icu/source/runConfigureICU").is_file() {
        fs::create_dir_all(&source_root)?;
        let archive = GzDecoder::new(BufReader::new(File::open(&archive_path)?));
        tar::Archive::new(archive).unpack(&source_root)?;
    }
    fs::create_dir_all(&build_root)?;
    fs::create_dir_all(prefix)?;

    let platform = if target.contains("apple-darwin") {
        "MacOSX"
    } else if target.contains("linux") {
        "Linux"
    } else {
        return Err(format!("unsupported ICU source target: {target}").into());
    };
    let configure = source_root.join("icu/source/runConfigureICU");
    let mut configure_command = Command::new(configure);
    configure_command
        .current_dir(&build_root)
        .arg(platform)
        .arg(format!("--prefix={}", prefix.display()))
        .args([
            "--enable-static",
            "--disable-shared",
            "--disable-extras",
            "--disable-icuio",
            "--disable-tests",
            "--disable-samples",
            "--with-data-packaging=static",
        ]);
    prepend_flag(&mut configure_command, "CFLAGS", "-fPIC");
    prepend_flag(&mut configure_command, "CXXFLAGS", "-fPIC");
    run(&mut configure_command, "configure ICU")?;

    let jobs = std::thread::available_parallelism().map_or(2, usize::from);
    run(
        Command::new("make")
            .current_dir(&build_root)
            .arg(format!("-j{jobs}")),
        "build ICU",
    )?;
    run(
        Command::new("make").current_dir(&build_root).arg("install"),
        "install ICU",
    )?;
    Ok(())
}

fn ensure_archive(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if path.is_file() && sha256(path)? == ICU_ARCHIVE_SHA256 {
        return Ok(());
    }

    download(path)?;
    let digest = sha256(path)?;
    if digest == ICU_ARCHIVE_SHA256 {
        Ok(())
    } else {
        Err(format!("ICU source checksum mismatch: {digest}").into())
    }
}

fn download(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let temporary_path = path.with_extension("download");
    let mut response = ureq::get(ICU_ARCHIVE_URL).call()?;
    let mut reader = response.body_mut().as_reader();
    let mut output = File::create(&temporary_path)?;
    io::copy(&mut reader, &mut output)?;
    fs::rename(temporary_path, path)?;
    Ok(())
}

fn sha256(path: &Path) -> io::Result<String> {
    let mut input = BufReader::new(File::open(path)?);
    let mut digest = Sha256::new();
    io::copy(&mut input, &mut digest)?;
    Ok(format!("{:x}", digest.finalize()))
}

fn prepend_flag(command: &mut Command, name: &str, flag: &str) {
    let value = env::var(name).map_or_else(|_| flag.to_owned(), |value| format!("{flag} {value}"));
    command.env(name, value);
}

fn run(command: &mut Command, action: &str) -> Result<(), Box<dyn std::error::Error>> {
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{action} failed with {status}").into())
    }
}
