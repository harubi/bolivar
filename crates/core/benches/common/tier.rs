use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchTier {
    Quick,
    Full,
}

pub fn bench_tier() -> BenchTier {
    match env::var("BOLIVAR_BENCH_TIER").as_deref() {
        Ok("full") => BenchTier::Full,
        _ => BenchTier::Quick,
    }
}
