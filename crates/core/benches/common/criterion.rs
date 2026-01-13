use criterion::Criterion;

#[cfg(target_os = "linux")]
use criterion_perf_events::Perf;
#[cfg(target_os = "linux")]
use perfcnt::linux::{HardwareEventType as Hardware, PerfCounterBuilderLinux as Builder};

#[cfg(target_os = "linux")]
pub type BenchCriterion = Criterion<Perf>;

#[cfg(not(target_os = "linux"))]
pub type BenchCriterion = Criterion;

#[cfg(target_os = "linux")]
pub fn bench_criterion() -> BenchCriterion {
    let perf = Perf::new(Builder::from_hardware_event(Hardware::CpuCycles));
    Criterion::default()
        .with_measurement(perf)
        .configure_from_args()
}

#[cfg(not(target_os = "linux"))]
pub fn bench_criterion() -> BenchCriterion {
    Criterion::default().configure_from_args()
}
