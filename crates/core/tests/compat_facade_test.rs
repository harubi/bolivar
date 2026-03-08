use std::fs;
use std::path::{Path, PathBuf};

fn rust_source_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read source directory") {
        let entry = entry.expect("directory entry");
        let path = entry.path();
        if path.is_dir() {
            rust_source_files(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

#[test]
fn internal_core_modules_do_not_import_compat_aliases() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let lib_rs = src_dir.join("lib.rs");
    let forbidden_paths = [
        "crate::pdfdocument::",
        "crate::pdfpage::",
        "crate::pdfinterp::",
    ];

    let mut files = Vec::new();
    rust_source_files(&src_dir, &mut files);
    files.sort();

    let mut offenders = Vec::new();
    for path in files {
        if path == lib_rs {
            continue;
        }

        let source = fs::read_to_string(&path).expect("read source file");
        for needle in forbidden_paths {
            for (line_idx, line) in source.lines().enumerate() {
                if line.contains(needle) {
                    let relative = path
                        .strip_prefix(env!("CARGO_MANIFEST_DIR"))
                        .expect("strip manifest dir");
                    offenders.push(format!(
                        "{}:{}: {}",
                        relative.display(),
                        line_idx + 1,
                        line.trim()
                    ));
                }
            }
        }
    }
    offenders.sort();

    assert!(
        offenders.is_empty(),
        "internal core modules should depend on concrete modules, not compat aliases:\n{}",
        offenders.join("\n")
    );
}
