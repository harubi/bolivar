//! Platform-specific byte SIMD type and lane count.

use std::simd::Simd;

#[cfg(all(target_arch = "x86_64", target_feature = "avx512bw"))]
pub(crate) const U8_LANES: usize = 64;

#[cfg(all(
    target_arch = "x86_64",
    target_feature = "avx2",
    not(target_feature = "avx512bw")
))]
pub(crate) const U8_LANES: usize = 32;

#[cfg(not(all(target_arch = "x86_64", target_feature = "avx2")))]
pub(crate) const U8_LANES: usize = 16;

pub(crate) type SimdU8 = Simd<u8, U8_LANES>;
