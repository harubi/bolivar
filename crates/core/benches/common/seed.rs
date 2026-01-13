use std::env;

pub fn bench_seed() -> u64 {
    env::var("BOLIVAR_BENCH_SEED")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0xC0FFEE)
}
