use criterion::Criterion;

#[cfg(all(target_os = "linux", any(target_arch = "x86", target_arch = "x86_64")))]
use criterion_perf_events::Perf;
#[cfg(all(target_os = "linux", any(target_arch = "x86", target_arch = "x86_64")))]
use perfcnt::linux::{HardwareEventType as Hardware, PerfCounterBuilderLinux as Builder};

#[cfg(all(target_os = "linux", any(target_arch = "x86", target_arch = "x86_64")))]
pub type BenchCriterion = Criterion<Perf>;

#[cfg(not(all(target_os = "linux", any(target_arch = "x86", target_arch = "x86_64"))))]
pub type BenchCriterion = Criterion;

#[cfg(all(target_os = "linux", any(target_arch = "x86", target_arch = "x86_64")))]
pub fn bench_criterion() -> BenchCriterion {
    let perf = Perf::new(Builder::from_hardware_event(Hardware::CpuCycles));
    Criterion::default()
        .with_measurement(perf)
        .configure_from_args()
}

#[cfg(not(all(target_os = "linux", any(target_arch = "x86", target_arch = "x86_64"))))]
pub fn bench_criterion() -> BenchCriterion {
    Criterion::default().configure_from_args()
}
