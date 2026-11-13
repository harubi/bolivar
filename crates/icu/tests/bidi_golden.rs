//! Pins the inverse bidi output of the vendored ICU subset.
//!
//! The expected digest was recorded against a full upstream ICU4C 78.3
//! build. A change to the vendored sources, the compiler flags, or the ICU
//! version must keep it, or explain why it moved.

use bolivar_icu::Bidi;

const EXPECTED_DIGEST: u64 = 0x4c0b_4f5c_7c07_3dd0;

/// Characters that exercise every bidi class the reorderer treats apart:
/// strong L/R/AL, European and Arabic digits and separators, neutrals,
/// nonspacing marks, explicit embeddings, isolates, and a surrogate pair.
const POOL: &[u16] = &[
    b'a' as u16,
    b'Z' as u16,
    b'1' as u16,
    b'9' as u16,
    b' ' as u16,
    b'.' as u16,
    b',' as u16,
    b'-' as u16,
    b'(' as u16,
    b')' as u16,
    b'[' as u16,
    b'+' as u16,
    b'%' as u16,
    b'#' as u16,
    b'/' as u16,
    b':' as u16,
    b'$' as u16,
    0x0009,
    0x00A0,
    0x0300,
    0x05B4,
    0x05D0,
    0x05D3,
    0x05E9,
    0x0627,
    0x0644,
    0x0645,
    0x064E,
    0x0651,
    0x0660,
    0x0661,
    0x06F5,
    0x061C,
    0x200E,
    0x200F,
    0x202A,
    0x202B,
    0x202C,
    0x2029,
    0x2066,
    0x2067,
    0x2069,
    0x20AC,
    0xFB1D,
    0xFE70,
    0xD83D,
    0xDE00,
];

/// Paragraph levels: LTR, RTL, `UBIDI_DEFAULT_LTR`, `UBIDI_DEFAULT_RTL`.
const LEVELS: [u8; 4] = [0, 1, 0xFE, 0xFF];

const CASES: usize = 200_000;

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 as u32
    }
}

struct Fnv(u64);

impl Fnv {
    fn add(&mut self, value: u64) {
        self.0 = (self.0 ^ value).wrapping_mul(0x0000_0100_0000_01B3);
    }
}

#[test]
fn inverse_reordering_matches_upstream_icu() {
    let mut bidi = Bidi::new().expect("ubidi_open failed");
    let mut random = XorShift(0x0139_408D_CBBF_7A44);
    let mut digest = Fnv(0xCBF2_9CE4_8422_2325);
    let mut source = [0_u16; 64];
    let mut destination = [0_u16; 256];
    let mut mapping = [0_i32; 64];

    for case in 0..CASES {
        let length = 1 + random.next() as usize % 63;
        for unit in &mut source[..length] {
            *unit = POOL[random.next() as usize % POOL.len()];
        }

        let output = bidi.inverse(
            &source[..length],
            LEVELS[case % LEVELS.len()],
            &mut destination,
            Some(&mut mapping[..length]),
        );

        match output {
            Some(written) => {
                digest.add(written as u64);
                for &unit in &destination[..written] {
                    digest.add(u64::from(unit));
                }
                for &index in &mapping[..length] {
                    digest.add(index as u32 as u64);
                }
            }
            None => digest.add(u64::MAX),
        }
    }

    assert_eq!(
        digest.0, EXPECTED_DIGEST,
        "inverse bidi output changed: {:#018x}",
        digest.0
    );
}
