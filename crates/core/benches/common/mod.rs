use std::env;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use criterion::measurement::Measurement;
use criterion::{BenchmarkGroup, Criterion, Throughput};
use serde::Deserialize;

#[cfg(all(feature = "perf-counters", target_os = "linux"))]
use criterion_perf_events::Perf;
#[cfg(all(feature = "perf-counters", target_os = "linux"))]
use perfcnt::linux::{HardwareEventType as Hardware, PerfCounterBuilderLinux as Builder};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchTier {
    Quick,
    Full,
}

impl BenchTier {
    pub fn from_env() -> Self {
        match env::var("BOLIVAR_BENCH_TIER").as_deref() {
            Ok("full") => Self::Full,
            _ => Self::Quick,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Full => "full",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupWeight {
    Light,
    Heavy,
}

#[derive(Debug, Clone)]
pub struct BenchConfig {
    pub tier: BenchTier,
    pub seed: u64,
    pub sample_size_light: usize,
    pub sample_size_heavy: usize,
    pub measurement_light: Duration,
    pub measurement_heavy: Duration,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FixtureExpect {
    pub min_pages: Option<usize>,
    pub min_text_len: Option<usize>,
    pub min_tables: Option<usize>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Fixture {
    pub id: String,
    pub path: String,
    pub tiers: Vec<String>,
    pub tags: Vec<String>,
    pub expect: Option<FixtureExpect>,
    pub bench_pages: Option<Vec<usize>>,
    pub sha256: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Manifest {
    pub version: u32,
    pub fixtures: Vec<Fixture>,
}

#[derive(Debug, Clone)]
pub struct LoadedFixture {
    pub meta: Fixture,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

#[cfg(all(feature = "perf-counters", target_os = "linux"))]
pub type BenchCriterion = Criterion<Perf>;

#[cfg(not(all(feature = "perf-counters", target_os = "linux")))]
pub type BenchCriterion = Criterion;

pub fn bench_config() -> BenchConfig {
    let tier = BenchTier::from_env();
    let seed = env::var("BOLIVAR_BENCH_SEED")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0xC0FFEE);
    let (sample_size_light, sample_size_heavy, measurement_light, measurement_heavy) = match tier {
        BenchTier::Quick => (20, 12, Duration::from_secs(3), Duration::from_secs(5)),
        BenchTier::Full => (30, 20, Duration::from_secs(5), Duration::from_secs(10)),
    };

    BenchConfig {
        tier,
        seed,
        sample_size_light,
        sample_size_heavy,
        measurement_light,
        measurement_heavy,
    }
}

pub fn configure_group<M: Measurement>(
    group: &mut BenchmarkGroup<'_, M>,
    cfg: &BenchConfig,
    weight: GroupWeight,
) {
    match weight {
        GroupWeight::Light => {
            group.sample_size(cfg.sample_size_light);
            group.measurement_time(cfg.measurement_light);
        }
        GroupWeight::Heavy => {
            group.sample_size(cfg.sample_size_heavy);
            group.measurement_time(cfg.measurement_heavy);
        }
    }
}

pub fn fixture_manifest() -> Manifest {
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
    let cfg = bench_config();
    let manifest = fixture_manifest();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let filter = env::var("BOLIVAR_BENCH_FILTER").ok();

    manifest
        .fixtures
        .into_iter()
        .filter(|fx| fx.tiers.iter().any(|t| t == cfg.tier.as_str()))
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
            LoadedFixture {
                meta: fx,
                path,
                bytes,
            }
        })
        .collect()
}

#[cfg(all(feature = "perf-counters", target_os = "linux"))]
pub fn bench_criterion() -> BenchCriterion {
    let perf = Perf::new(Builder::from_hardware_event(Hardware::CpuCycles));
    Criterion::default()
        .with_measurement(perf)
        .configure_from_args()
}

#[cfg(not(all(feature = "perf-counters", target_os = "linux")))]
pub fn bench_criterion() -> BenchCriterion {
    Criterion::default().configure_from_args()
}

pub fn bytes_throughput(len: usize) -> Throughput {
    Throughput::Bytes(len as u64)
}

pub fn pages_throughput(pages: usize) -> Throughput {
    Throughput::Elements(pages as u64)
}

#[derive(Clone)]
pub struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    pub fn gen_f64(&mut self, min: f64, max: f64) -> f64 {
        let n = self.next_u64() as f64 / u64::MAX as f64;
        min + (max - min) * n
    }
}
