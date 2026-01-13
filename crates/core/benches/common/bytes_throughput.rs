use criterion::Throughput;

pub fn bytes_throughput(len: usize) -> Throughput {
    Throughput::Bytes(len as u64)
}
