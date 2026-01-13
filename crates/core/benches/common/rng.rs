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
