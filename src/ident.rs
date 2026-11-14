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
#[derive(Clone, Copy, Hash, Serialize, Deserialize)]
pub struct GhostId([u8; 32]);

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

// Equality and order are those of the byte array. They are written out
// over four words so that they compile to a few inline compares; the
// derived versions call `memcmp`, which showed as a tenth of the run
// time of a cascade (match keys, participation lists, map lookups).
impl PartialEq for GhostId {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.words() == other.words()
    }
}

impl Eq for GhostId {}

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
