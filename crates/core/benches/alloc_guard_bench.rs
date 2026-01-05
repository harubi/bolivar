use criterion::{Criterion, criterion_group, criterion_main};

fn alloc_guard_bench(_c: &mut Criterion) {}

criterion_group!(benches, alloc_guard_bench);
criterion_main!(benches);
