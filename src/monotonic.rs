//! Monotonic (collision-free in-process) generation.
//!
//! Same 64-bit format as the stateless generator. Per tag, one atomic holds
//! the last issued `(timestamp, random)`:
//!
//! - new second → fresh CSPRNG random field;
//! - same second → previous random + 1;
//! - random field exhausted within a second → the timestamp borrows the next
//!   second with a fresh random field (non-blocking; the clock catches up).
//!   At 26 random bits (default profile) this needs > 2^26 / 2 ≈ 33M
//!   IDs/s/tag on average; bursts can move logical time ahead.
//!
//! Past the profile's last encodable second (see [`crate::type_bits`]) the
//! infallible [`Sequencer::generate`] wraps the timestamp like 0.5.x;
//! [`Sequencer::try_generate`] returns `None` instead.
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
use crate::type_bits::{IDTYPE_MASK, RANDOM_MASK, TIMESTAMP_MASK, encode_svid};

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

    /// Next ID for `id_type` (0..=127; higher bits are masked off) using the
    /// system clock.
    #[inline]
    pub fn generate(&self, id_type: u8, is_client: bool) -> i64 {
        self.generate_at(id_type, is_client, SvidGenerator::current_timestamp())
    }

    /// Next ID for `id_type` with an explicit clock reading (seconds since
    /// [`SVID_EPOCH`](crate::SVID_EPOCH)).
    #[inline]
    pub fn generate_at(&self, id_type: u8, is_client: bool, now: u32) -> i64 {
        self.next_with(id_type, is_client, now, SvidGenerator::random_field)
    }

    /// Checked [`generate`](Self::generate): `None` if `id_type > 127` or the
    /// timestamp is past the profile's end of life.
    #[inline]
    pub fn try_generate(&self, id_type: u8, is_client: bool) -> Option<i64> {
        self.try_generate_at(id_type, is_client, SvidGenerator::current_timestamp())
    }

    /// Checked [`generate_at`](Self::generate_at).
    #[inline]
    pub fn try_generate_at(&self, id_type: u8, is_client: bool, now: u32) -> Option<i64> {
        if id_type as i64 > IDTYPE_MASK || now as i64 > TIMESTAMP_MASK {
            return None;
        }
        let (ts, r) = self.next_parts(id_type, now, SvidGenerator::random_field);
        (ts as i64 <= TIMESTAMP_MASK).then(|| encode_svid(ts, is_client, id_type, r))
    }

    #[inline]
    pub(crate) fn next_with(
        &self,
        id_type: u8,
        is_client: bool,
        now: u32,
        rng: impl FnMut() -> u32,
    ) -> i64 {
        debug_assert!(
            id_type as i64 <= IDTYPE_MASK,
            "id_type {id_type} exceeds 7-bit range (0..=127)"
        );
        let tag = id_type & IDTYPE_MASK as u8;
        let (ts, r) = self.next_parts(tag, now, rng);
        encode_svid(ts, is_client, tag, r)
    }

    /// `(timestamp, random)` for the next ID of `tag` (already masked).
    #[inline]
    fn next_parts(&self, tag: u8, now: u32, mut rng: impl FnMut() -> u32) -> (u32, u32) {
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
            return (lts, lr + 1);
        }
        let mut cur = prev.wrapping_add(1);
        loop {
            let (lts, lr) = ((cur >> 32) as u32, cur as u32);
            let (ts, r) = if cur == 0 || now > lts {
                (now, rng() & RANDOM_MASK as u32)
            } else if lr < RANDOM_MASK as u32 {
                (lts, lr + 1)
            } else {
                (lts.wrapping_add(1), rng() & RANDOM_MASK as u32)
            };
            match slot.compare_exchange_weak(cur, ((ts as u64) << 32) | r as u64, Relaxed, Relaxed)
            {
                Ok(_) => {
                    if ts > now {
                        self.last_ts.fetch_max(ts, Relaxed);
                    }
                    return (ts, r);
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

    #[test]
    fn try_generate_rejects_out_of_range_without_panicking() {
        let s = Sequencer::new();
        let max = TIMESTAMP_MASK as u32;
        assert!(s.try_generate_at(128, false, 100).is_none());
        assert!(s.try_generate_at(1, false, max + 1).is_none());
        let id = s.try_generate_at(127, true, max).unwrap();
        assert_eq!(
            (id.tag(), id.timestamp_bits(), id.is_client()),
            (127, max, true)
        );
        // Exhausting the last second would borrow past end of life: None, not a wrap.
        let mut saw_none = false;
        for _ in 0..=(MASK as u64 + 1) {
            if s.try_generate_at(127, true, max).is_none() {
                saw_none = true;
                break;
            }
        }
        assert!(saw_none);
        // The infallible path does not panic past end of life; it wraps like 0.5.x.
        let _ = s.generate_at(1, false, u32::MAX);
    }

    #[test]
    fn sequencer_is_small() {
        assert!(std::mem::size_of::<Sequencer>() <= 128 * 64 + 64);
    }
}
