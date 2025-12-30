//! Arcfour (RC4) stream cipher wrapper.
//!
//! Port of pdfminer.six Arcfour class.
//! RC4 implementation supporting variable-length keys (1-256 bytes).

/// RC4 stream cipher.
pub struct Arcfour {
    state: [u8; 256],
    i: u8,
    j: u8,
}

impl Arcfour {
    /// Create new Arcfour cipher with key.
    ///
    /// Key must be 1-256 bytes.
    pub fn new(key: &[u8]) -> Self {
        assert!(
            !key.is_empty() && key.len() <= 256,
            "RC4 key must be 1-256 bytes"
        );

        let mut state: [u8; 256] = std::array::from_fn(|i| i as u8);

        // Key-scheduling algorithm (KSA)
        let mut j: u8 = 0;
        for i in 0..256 {
            j = j.wrapping_add(state[i]).wrapping_add(key[i % key.len()]);
            state.swap(i, j as usize);
        }

        Self { state, i: 0, j: 0 }
    }

    /// Encrypt/decrypt data (RC4 is symmetric).
    pub fn process(&mut self, data: &[u8]) -> Vec<u8> {
        data.iter().map(|byte| byte ^ self.prga()).collect()
    }

    /// Pseudo-random generation algorithm (PRGA).
    fn prga(&mut self) -> u8 {
        self.i = self.i.wrapping_add(1);
        self.j = self.j.wrapping_add(self.state[self.i as usize]);
        self.state.swap(self.i as usize, self.j as usize);

        let idx = self.state[self.i as usize].wrapping_add(self.state[self.j as usize]);
        self.state[idx as usize]
    }
}
