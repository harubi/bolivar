use std::env;
use std::path::Path;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::bench_tier::{BenchTier, bench_tier};

#[derive(Debug, Deserialize, Clone)]
pub struct Fixture {
    pub id: String,
    pub path: String,
    pub tiers: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Manifest {
    pub fixtures: Vec<Fixture>,
}

#[derive(Debug, Clone)]
pub struct LoadedFixture {
    pub meta: Fixture,
    pub bytes: Vec<u8>,
}

fn tier_str(tier: BenchTier) -> &'static str {
    match tier {
        BenchTier::Quick => "quick",
        BenchTier::Full => "full",
    }
}

fn fixture_manifest() -> Manifest {
    static MANIFEST: OnceLock<Manifest> = OnceLock::new();
    MANIFEST
        .get_or_init(|| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
            let path = root.join("benchmarks/fixtures.json");
            let data = std::fs::read(&path).expect("fixtures.json missing");
            serde_json::from_slice(&data).expect("fixtures.json invalid")
        })
        .clone()
}

pub fn load_fixtures(tag: Option<&str>) -> Vec<LoadedFixture> {
    let tier = bench_tier();
    let manifest = fixture_manifest();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let filter = env::var("BOLIVAR_BENCH_FILTER").ok();
    let tier = tier_str(tier);

    manifest
        .fixtures
        .into_iter()
        .filter(|fx| fx.tiers.iter().any(|t| t == tier))
        .filter(|fx| {
            tag.map(|t| fx.tags.iter().any(|tag| tag == t))
                .unwrap_or(true)
        })
        .filter(|fx| {
            filter
                .as_ref()
                .map(|needle| fx.id.contains(needle))
                .unwrap_or(true)
        })
        .map(|fx| {
            let path = root.join(&fx.path);
            let bytes = std::fs::read(&path)
                .unwrap_or_else(|e| panic!("failed to read fixture {}: {}", path.display(), e));
            LoadedFixture { meta: fx, bytes }
        })
        .collect()
}
