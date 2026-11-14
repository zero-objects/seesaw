//! Identity and lifecycle state: the two types every layer shares.
//!
//! Rescued from the first generation's removed graph module, reduced
//! to what's used here. Identity derivation itself lives in `graph`,
//! GhostId is a pure carrier here.

use serde::{Deserialize, Serialize};

use std::fmt;

/// Status of an element in the ghost graph.
///
/// See Def. 2.4 in the paper.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Status {
    /// Element from the baseline L_0/R_0, untouched.
    Solid,
    /// Virtually added, not yet materialized.
    Ghost,
    /// **M5.5**: Tentative tombstone during a cascade
    /// invalidation phase. Resolves in the consolidation phase
    /// either back to `Solid` (resurrection via identical
    /// Ghost-ID of a new rule application) or to `Tombstone`
    /// (final invalidation).
    ///
    /// Remains matchable (see `Status::is_matchable`) so that a
    /// new rule application can reclaim the identity.
    TentativeTombstone,
    /// Virtually deleted; invisible to pattern matching,
    /// visible to conflict detection and V₁₂ induction.
    Tombstone,
}

impl Status {
    /// Elements visible to pattern matching.
    ///
    /// Solid + Ghost as usual. **TentativeTombstone** is deliberately
    /// matchable (M5.5): during the consolidation phase a new rule
    /// application may reclaim the element via the same Ghost-ID
    /// (resurrection).
    pub fn is_matchable(&self) -> bool {
        matches!(
            self,
            Status::Solid | Status::Ghost | Status::TentativeTombstone
        )
    }
}

/// Parent-rooted Ghost-ID (blake3 hash).
///
/// See Def. 5.3 in the paper. 32-byte blake3 hash over the cascade
/// history of an element. blake3 is cryptographically collision-resistant
/// (so the structural identity contract of Def. 5.3 holds — distinct
/// content never aliases) while being SIMD-accelerated, unlike the prior
/// SHA-256 software fallback (rc10 perf).
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GhostId([u8; 32]);

// An identity is the output of a cryptographic hash, so any eight of
// its bytes are as good a hash of it as all 32. Feeding the maps one
// word instead of the array (length and four words through an
// un-inlined call) takes that cost out of every lookup. The first and
// the last word are combined so that an identity a host built by hand
// (`from_raw`) with a constant head still spreads.
impl std::hash::Hash for GhostId {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let w = self.words();
        state.write_u64(w[0] ^ w[3]);
    }
}

impl GhostId {
    /// The 32 bytes as four big-endian words. Comparing the words in
    /// order is comparing the bytes in order.
    #[inline]
    fn words(&self) -> [u64; 4] {
        let mut w = [0u64; 4];
        for (i, word) in w.iter_mut().enumerate() {
            let mut b = [0u8; 8];
            b.copy_from_slice(&self.0[i * 8..i * 8 + 8]);
            *word = u64::from_be_bytes(b);
        }
        w
    }
}

// The order is that of the byte array, written out over four words so
// that it compiles to inline compares; the derived version calls
// `memcmp`. Equality stays derived: written out over the words it
// measured slower than the derived compare of the 32 bytes.
impl Ord for GhostId {
    #[inline]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let (a, b) = (self.words(), other.words());
        for i in 0..4 {
            if a[i] != b[i] {
                return a[i].cmp(&b[i]);
            }
        }
        std::cmp::Ordering::Equal
    }
}

impl PartialOrd for GhostId {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl GhostId {
    /// Short hexadecimal form (8 characters) for user interfaces and logging.
    pub fn short(&self) -> String {
        format!(
            "{:02x}{:02x}{:02x}{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3]
        )
    }

    /// Full 64-character hex form (32 bytes). Lossless, so a host can
    /// carry an identity out and hand it back unchanged.
    pub fn hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for b in &self.0 {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
        }
        s
    }

    /// Raw 32-byte hash.
    pub fn as_bytes(&self) -> [u8; 32] {
        self.0
    }

    /// Constructs from raw bytes (identity derivation; domain
    /// separation is the caller's responsibility).
    pub fn from_raw(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl fmt::Debug for GhostId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GhostId({})", self.short())
    }
}

#[cfg(test)]
mod tests {
    use super::GhostId;
    use proptest::prelude::*;

    proptest! {
        /// Equality and order of `GhostId` are those of its bytes.
        #[test]
        fn order_is_the_byte_order(a in any::<[u8; 32]>(), b in any::<[u8; 32]>(), cut in 0usize..32) {
            // Also pairs with a common prefix, which random arrays
            // almost never have.
            let mut c = b;
            c[..cut].copy_from_slice(&a[..cut]);
            for (x, y) in [(a, b), (a, c), (a, a)] {
                let (gx, gy) = (GhostId::from_raw(x), GhostId::from_raw(y));
                prop_assert_eq!(gx.cmp(&gy), x.cmp(&y));
                prop_assert_eq!(gx == gy, x == y);
            }
        }
    }
}
