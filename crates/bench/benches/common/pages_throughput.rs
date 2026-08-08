use criterion::Throughput;

pub fn pages_throughput(pages: usize) -> Throughput {
    Throughput::Elements(pages as u64)
}
