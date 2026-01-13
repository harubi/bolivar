use std::time::Duration;

use criterion::measurement::Measurement;
use criterion::BenchmarkGroup;

use crate::bench_tier::BenchTier;

pub fn configure_group_heavy<M: Measurement>(group: &mut BenchmarkGroup<'_, M>, tier: BenchTier) {
    let (sample_size, measurement) = match tier {
        BenchTier::Quick => (12, Duration::from_secs(5)),
        BenchTier::Full => (20, Duration::from_secs(10)),
    };
    group.sample_size(sample_size);
    group.measurement_time(measurement);
}
