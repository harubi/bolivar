//! Platform-optimal SIMD types and lane counts.
//!
//! Tier 1: AVX-512 (x86_64) - 512-bit vectors
//! Tier 2: AVX/AVX2 (x86_64) - 256-bit vectors
//! Tier 3: ARM NEON / SSE2 / fallback - 128-bit vectors

use std::simd::Simd;
use std::simd::prelude::*;

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

pub(crate) type SimdF64 = Simd<f64, F64_LANES>;

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

pub(crate) fn find_subslice_simd(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > haystack.len() {
        return None;
    }

    let first = needle[0];
    let max_start = haystack.len() - needle.len();
    let mut i = 0usize;
    let prefix_len = (max_start + 1).min(8);
    while i < prefix_len {
        if haystack[i] == first && &haystack[i..i + needle.len()] == needle {
            return Some(i);
        }
        i += 1;
    }

    let search_len = max_start + 1;
    if search_len - i < U8_LANES {
        while i < search_len {
            if haystack[i] == first && &haystack[i..i + needle.len()] == needle {
                return Some(i);
            }
            i += 1;
        }
        return None;
    }

    let data = &haystack[i..search_len];
    let (prefix, middle, suffix) = data.as_simd::<{ U8_LANES }>();

    let mut offset = i;
    for (idx, &b) in prefix.iter().enumerate() {
        if b == first && &haystack[offset + idx..offset + idx + needle.len()] == needle {
            return Some(offset + idx);
        }
    }
    offset += prefix.len();

    let needle_vec = SimdU8::splat(first);
    for chunk in middle.iter() {
        let mut mask = chunk.simd_eq(needle_vec).to_bitmask();
        while mask != 0 {
            let bit = mask.trailing_zeros() as usize;
            let pos = offset + bit;
            if &haystack[pos..pos + needle.len()] == needle {
                return Some(pos);
            }
            mask &= mask - 1;
        }
        offset += U8_LANES;
    }

    for (idx, &b) in suffix.iter().enumerate() {
        if b == first && &haystack[offset + idx..offset + idx + needle.len()] == needle {
            return Some(offset + idx);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::find_subslice_simd;

    #[test]
    fn find_subslice_simd_basic() {
        assert_eq!(find_subslice_simd(b"hello world", b"world"), Some(6));
        assert_eq!(find_subslice_simd(b"hello world", b"nope"), None);
        assert_eq!(find_subslice_simd(b"abc", b""), Some(0));
        assert_eq!(find_subslice_simd(b"aaaaa", b"aa"), Some(0));
    }
}
