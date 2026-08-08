use std::fs;
use std::path::{Path, PathBuf};

fn collect_rust_files(root: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(root).expect("read source directory");
    for entry in entries {
        let entry = entry.expect("read source entry");
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, out);
            continue;
        }
        if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn internal_core_modules_do_not_import_compat_aliases() {
    let src_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut rust_files = Vec::new();
    collect_rust_files(&src_root, &mut rust_files);
    rust_files.sort();

    let mut violations = Vec::new();

    for path in rust_files {
        if path.file_name().is_some_and(|name| name == "lib.rs") {
            continue;
        }

        let relative = path
            .strip_prefix(&src_root)
            .expect("source file under src")
            .display()
            .to_string();
        let content = fs::read_to_string(&path).expect("read source file");
        for (line_index, line) in content.lines().enumerate() {
            if !line.contains("use ") {
                continue;
            }
            if line.contains("pdfdocument::")
                || line.contains("pdfpage::")
                || line.contains("pdfinterp::")
            {
                violations.push(format!("{}:{}", relative, line_index + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "internal core modules must not import compat aliases:\n{}",
        violations.join("\n")
    );
}
