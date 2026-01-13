use std::path::Path;

use sha2::{Digest, Sha256};

#[derive(serde::Deserialize)]
struct Fixture {
    id: String,
    path: String,
    tiers: Vec<String>,
    tags: Vec<String>,
    sha256: Option<String>,
}

#[derive(serde::Deserialize)]
struct Manifest {
    version: u32,
    fixtures: Vec<Fixture>,
}

#[test]
fn test_bench_fixtures_load() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let manifest_path = root.join("benchmarks/fixtures.json");
    let data = std::fs::read(&manifest_path).expect("fixtures.json missing");
    let manifest: Manifest = serde_json::from_slice(&data).expect("fixtures.json invalid");

    assert_eq!(manifest.version, 1);
    assert!(!manifest.fixtures.is_empty());

    for fixture in &manifest.fixtures {
        let fpath = root.join(&fixture.path);
        assert!(fpath.exists(), "fixture missing: {}", fpath.display());
        assert!(!fixture.id.trim().is_empty());
        assert!(!fixture.tiers.is_empty());
        assert!(!fixture.tags.is_empty());

        if let Some(expected) = &fixture.sha256 {
            let bytes = std::fs::read(&fpath).expect("fixture read failed");
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let digest = hex::encode(hasher.finalize());
            assert_eq!(expected, &digest, "sha256 mismatch for {}", fixture.id);
        }
    }
}
