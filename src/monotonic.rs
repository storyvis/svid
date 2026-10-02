//! Monotonic (collision-free in-process) generation.
//!
//! Same 64-bit format as the stateless generator. Per tag, one atomic holds
//! the last issued `(timestamp, random)`:
//!
//! - new second → fresh CSPRNG random field;
//! - same second → previous random + 1;
//! - random field exhausted within a second → the timestamp borrows the next
//!   second with a fresh random field (non-blocking; the clock catches up).
//!   With the default 19 random bits, each fresh second has about 262K
//!   remaining values on average; bursts can move logical time ahead.
//!
//! Clock clamp: the timestamp never goes below the highest timestamp already
//! issued by this [`Sequencer`] (any tag), so a backwards `SystemTime` step
//! cannot break `ORDER BY id`. IDs keep the last issued second until the
//! wall clock passes it again.
//!
//! In-process: zero collisions, and IDs of one tag are strictly increasing in
//! issue order. Across processes, the expected number of colliding pairs is
//! the same as for stateless IDs (each cross-process pair still matches with
//! probability 2^-M); collisions are just clustered. Sequential random fields
//! are predictable — never treat an SVID as a secret.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};

use crate::SvidGenerator;
use crate::type_bits::{RANDOM_MASK, encode_svid};

#[repr(align(64))]
struct Slot(AtomicU64);

/// Per-tag monotonic state. Use [`Sequencer::global`] unless you need an
/// isolated sequence (e.g. tests with an injected clock).
pub struct Sequencer {
    slots: [Slot; 1 << crate::type_bits::IDTYPE_BITS],
    last_ts: AtomicU32,
}

static GLOBAL: Sequencer = Sequencer::new();

impl Default for Sequencer {
    fn default() -> Self {
        Self::new()
    }
}

impl Sequencer {
    pub const fn new() -> Self {
        Self {
            slots: [const { Slot(AtomicU64::new(0)) }; 1 << crate::type_bits::IDTYPE_BITS],
            last_ts: AtomicU32::new(0),
        }
    }

    /// The process-wide sequencer used by [`SvidGenerator::generate_monotonic`].
    #[inline]
    pub fn global() -> &'static Sequencer {
        &GLOBAL
    }

    /// Next ID for `id_type` using the system clock.
    #[inline]
    pub fn generate(&self, id_type: u16, is_client: bool) -> i64 {
        self.generate_at(id_type, is_client, SvidGenerator::current_timestamp())
    }

    /// Next ID for `id_type` with an explicit clock reading (seconds since
    /// [`SVID_EPOCH`](crate::SVID_EPOCH)).
    #[inline]
    pub fn generate_at(&self, id_type: u16, is_client: bool, now: u32) -> i64 {
        self.next_with(id_type, is_client, now, SvidGenerator::random_field)
    }

    pub(crate) fn next_with(
        &self,
        id_type: u16,
        is_client: bool,
        now: u32,
        mut rng: impl FnMut() -> u32,
    ) -> i64 {
        assert!(
            id_type <= 4095,
            "id_type {} exceeds 12-bit range (0..=4095)",
            id_type
        );
        assert!(
            now as i64 <= crate::TIMESTAMP_MASK,
            "SVID64 timestamp exhausted"
        );
        let tag = id_type;

        let last = self.last_ts.load(Relaxed);
        let now = if now > last {
            self.last_ts.fetch_max(now, Relaxed).max(now)
        } else {
            last
        };

        let slot = &self.slots[tag as usize].0;
        // Fast path (same second, room left): one fetch_add, no CAS retries
        // under contention. Otherwise the increment is just a skipped value
        // and the CAS loop below reseeds/borrows; it treats a random field
        // pushed past the mask by concurrent fetch_adds as overflow.
        let prev = slot.fetch_add(1, Relaxed);
        let (lts, lr) = ((prev >> 32) as u32, prev as u32);
        if prev != 0 && now <= lts && lr < RANDOM_MASK as u32 {
            return encode_svid(lts, is_client, tag, lr + 1);
        }
        let mut cur = prev.wrapping_add(1);
        loop {
            let (lts, lr) = ((cur >> 32) as u32, cur as u32);
            let (ts, r) = if cur == 0 || now > lts {
                (now, rng() & RANDOM_MASK as u32)
            } else if lr < RANDOM_MASK as u32 {
                (lts, lr + 1)
            } else {
                (lts + 1, rng() & RANDOM_MASK as u32)
            };
            assert!(
                ts as i64 <= crate::TIMESTAMP_MASK,
                "SVID64 timestamp exhausted"
            );
            match slot.compare_exchange_weak(cur, ((ts as u64) << 32) | r as u64, Relaxed, Relaxed)
            {
                Ok(_) => {
                    if ts > now {
                        self.last_ts.fetch_max(ts, Relaxed);
                    }
                    return encode_svid(ts, is_client, tag, r);
                }
                Err(actual) => cur = actual,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SvidExt;

    const MASK: u32 = RANDOM_MASK as u32;

    #[test]
    fn increments_within_second_and_reseeds_on_new_second() {
        let s = Sequencer::new();
        let a = s.next_with(1, false, 100, || 5);
        let b = s.next_with(1, false, 100, || unreachable!());
        assert_eq!((a.timestamp_bits(), a.random_bits()), (100, 5));
        assert_eq!((b.timestamp_bits(), b.random_bits()), (100, 6));
        let c = s.next_with(1, false, 101, || 3);
        assert_eq!((c.timestamp_bits(), c.random_bits()), (101, 3));
    }

    #[test]
    fn overflow_borrows_next_second() {
        let s = Sequencer::new();
        let a = s.next_with(1, false, 100, || MASK - 1);
        let b = s.next_with(1, false, 100, || unreachable!());
        let c = s.next_with(1, false, 100, || 7);
        let d = s.next_with(1, false, 100, || unreachable!());
        assert_eq!((a.timestamp_bits(), a.random_bits()), (100, MASK - 1));
        assert_eq!((b.timestamp_bits(), b.random_bits()), (100, MASK));
        assert_eq!((c.timestamp_bits(), c.random_bits()), (101, 7));
        assert_eq!((d.timestamp_bits(), d.random_bits()), (101, 8));
        assert!(a < b && b < c && c < d);
        // The borrowed second also clamps other tags.
        let e = s.next_with(2, false, 100, || 0);
        assert_eq!(e.timestamp_bits(), 101);
        // Wall clock catching up continues the sequence, then reseeds.
        let f = s.next_with(1, false, 101, || unreachable!());
        assert_eq!((f.timestamp_bits(), f.random_bits()), (101, 9));
        let g = s.next_with(1, false, 102, || 1);
        assert_eq!((g.timestamp_bits(), g.random_bits()), (102, 1));
    }
}
