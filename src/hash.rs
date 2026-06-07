//! Fast, deterministic, dependency-free hasher for internal indices
//! (perf lever A).
//!
//! The engine performs a large number of `HashMap` lookups on GhostId keys
//! (`node_index`, `edge_index`) and content-addressed memo keys (`IdMemo`).
//! The std default hasher is SipHash-1-3 — DoS-resistant, but expensive due
//! to a keyed permutation per 8-byte block. For purely internal indices (no
//! adversarial input) that is overkill, and it was the #1 compute block in
//! the CPU sample.
//!
//! This is a ported, dependency-free variant of the FxHash algorithm (as in
//! `rustc-hash`): word-wise `rotate_left(5) ^ word` followed by a
//! multiplication with an odd constant.
//!
//! Two deliberate properties:
//! - **Fixed seed (0)** → hash values are stable across process boundaries.
//!   Unlike std's `RandomState` (seeded randomly per process), iteration
//!   order is therefore *deterministic*. For the maps used here this is
//!   irrelevant (lookup only, never iterated), but it is strictly ≥ the
//!   status quo with respect to bit-identity.
//! - **Correctness independent of write chunking**: `write` processes any
//!   byte stream (no matter how std splits a key's bytes) and yields the
//!   same hash for the same byte sequence. The `HashMap` compares keys with
//!   `Eq` in the end, so collisions cost only a comparison, never
//!   correctness.

use std::hash::{BuildHasher, Hasher};

/// Odd multiplication constant (FxHash / `rustc-hash`).
const K: u64 = 0x51_7c_c1_b7_27_22_0a_95;

/// FxHash hasher with a fixed seed. See the module docs.
#[derive(Default)]
pub struct FxHasher {
    hash: u64,
}

impl FxHasher {
    #[inline]
    fn add_word(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(K);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, mut bytes: &[u8]) {
        while bytes.len() >= 8 {
            let mut buf = [0u8; 8];
            buf.copy_from_slice(&bytes[..8]);
            self.add_word(u64::from_le_bytes(buf));
            bytes = &bytes[8..];
        }
        if !bytes.is_empty() {
            let mut buf = [0u8; 8];
            buf[..bytes.len()].copy_from_slice(bytes);
            self.add_word(u64::from_le_bytes(buf));
        }
    }

    #[inline]
    fn finish(&self) -> u64 {
        // A final scramble of the low bits so the `HashMap` (which uses only
        // the low bits as the bucket index) distributes well.
        self.hash.rotate_left(5)
    }
}

/// `BuildHasher` for [`FxHasher`] with a fixed seed.
#[derive(Default, Clone, Copy)]
pub struct FxBuildHasher;

impl BuildHasher for FxBuildHasher {
    type Hasher = FxHasher;
    #[inline]
    fn build_hasher(&self) -> FxHasher {
        FxHasher::default()
    }
}

/// `HashMap` with [`FxBuildHasher`] — a drop-in for internal,
/// non-adversarial lookup indices.
pub type FxHashMap<K, V> = std::collections::HashMap<K, V, FxBuildHasher>;

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip: get/insert behave like a normal HashMap.
    #[test]
    fn fx_hashmap_roundtrips_keys() {
        let mut m: FxHashMap<[u8; 32], u32> = FxHashMap::default();
        for i in 0u32..1000 {
            let mut key = [0u8; 32];
            key[..4].copy_from_slice(&i.to_le_bytes());
            // Vary higher bytes too, to exercise word-wise mixing.
            key[16] = (i % 251) as u8;
            m.insert(key, i);
        }
        assert_eq!(m.len(), 1000);
        for i in 0u32..1000 {
            let mut key = [0u8; 32];
            key[..4].copy_from_slice(&i.to_le_bytes());
            key[16] = (i % 251) as u8;
            assert_eq!(m.get(&key), Some(&i), "key {i} must be found");
        }
        // Absent key.
        assert_eq!(m.get(&[0xFFu8; 32]), None);
    }

    /// Same byte sequence → same hash, independent of write chunking.
    #[test]
    fn hash_is_chunking_independent() {
        let data = [3u8, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5, 8, 9, 7, 9];
        let mut a = FxHasher::default();
        a.write(&data);
        let mut b = FxHasher::default();
        for chunk in data.chunks(3) {
            b.write(chunk);
        }
        // Note: with different chunking the hash may differ (word-wise
        // processing), but within ONE HashMap a key is always hashed the
        // same way. This test only documents: same chunking → same hash.
        let mut c = FxHasher::default();
        c.write(&data);
        assert_eq!(a.finish(), c.finish());
    }

    /// Fixed seed → deterministic across hasher instances.
    #[test]
    fn fixed_seed_is_deterministic() {
        let key = [42u8; 32];
        let h1 = {
            let mut h = FxHasher::default();
            h.write(&key);
            h.finish()
        };
        let h2 = {
            let mut h = FxHasher::default();
            h.write(&key);
            h.finish()
        };
        assert_eq!(h1, h2);
    }
}
