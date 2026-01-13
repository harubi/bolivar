//! Platform-optimal SIMD types and lane counts.
//!
//! Tier 1: AVX-512 (x86_64) - 512-bit vectors
//! Tier 2: AVX/AVX2 (x86_64) - 256-bit vectors
//! Tier 3: ARM NEON / SSE2 / fallback - 128-bit vectors

use std::simd::Simd;

// --- f64: AVX-512 = 8 lanes, AVX = 4 lanes, else = 2 lanes ---

#[cfg(all(target_arch = "x86_64", target_feature = "avx512f"))]
pub(crate) const F64_LANES: usize = 8;

#[cfg(all(
    target_arch = "x86_64",
    target_feature = "avx",
    not(target_feature = "avx512f")
))]
pub(crate) const F64_LANES: usize = 4;

#[cfg(not(all(target_arch = "x86_64", target_feature = "avx")))]
pub(crate) const F64_LANES: usize = 2;

// --- u8: AVX-512 = 64 lanes, AVX2 = 32 lanes, else = 16 lanes ---

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
