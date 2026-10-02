//! 128-bit SVID for distributed tracing (W3C `traceparent` trace-id).
//!
//! ```text
//!  127                 87 86      80 79                                0
//! ┌─────────────────────┬──────────┬───────────────────────────────────┐
//! │   MILLIS (41 bits)  │ TAG (7b) │        RANDOM (80 bits, CSPRNG)   │
//! └─────────────────────┴──────────┴───────────────────────────────────┘
//! ```
//!
//! - **Millis**: milliseconds since [`SVID_EPOCH`] (2026-01-01), 41 bits ≈ 69.7
//!   years (wraps in 2095). Top of the value, so IDs sort by creation time.
//! - **Tag**: same 7-bit tag space as the 64-bit SVID.
//! - **Random**: 80 CSPRNG bits in the low 10 bytes, which satisfies the W3C
//!   Trace Context Level 2 "random trace-id" requirement (rightmost 7 bytes
//!   random). 50% collision needs ~1.3 × 10^12 IDs in the same millisecond.
//! - Never zero (W3C forbids the all-zero trace-id); enforced by
//!   [`NonZeroU128`].

use std::num::NonZeroU128;

use crate::encoding::{decode_hex_exact, encode_hex16_into, hex_into};
use crate::type_bits::{IDTYPE_MASK, RANDOM_ID_TAG, SVID_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Svid128(NonZeroU128);

impl Svid128 {
    pub const MILLIS_BITS: u32 = 41;
    pub const TAG_BITS: u32 = 7;
    pub const RANDOM_BITS: u32 = 80;
    pub const TAG_SHIFT: u32 = Self::RANDOM_BITS;
    pub const MILLIS_SHIFT: u32 = Self::TAG_SHIFT + Self::TAG_BITS;
    pub const RANDOM_MASK: u128 = (1 << Self::RANDOM_BITS) - 1;
    pub const MILLIS_MASK: u64 = (1 << Self::MILLIS_BITS) - 1;
    /// Length of [`to_hex`](Self::to_hex) output.
    pub const HEX_LEN: usize = 32;

    /// Fresh ID with the given tag (0..=127).
    pub fn generate(tag: u8) -> Self {
        use rand::Rng;
        let random = rand::rng().random::<u128>();
        let v = Self::pack(now_millis(), tag, random);
        // Only reachable before SVID_EPOCH with tag 0 and 2^-80 luck.
        Self(NonZeroU128::new(v).unwrap_or(NonZeroU128::MIN))
    }

    /// Untyped trace ID carrying [`RANDOM_ID_TAG`].
    pub fn generate_random() -> Self {
        Self::generate(RANDOM_ID_TAG)
    }

    /// Pack explicit fields. `None` only if every field is zero.
    pub fn from_parts(millis: u64, tag: u8, random: u128) -> Option<Self> {
        Self::from_u128(Self::pack(millis, tag, random))
    }

    #[inline]
    fn pack(millis: u64, tag: u8, random: u128) -> u128 {
        ((millis & Self::MILLIS_MASK) as u128) << Self::MILLIS_SHIFT
            | ((tag as u128) & IDTYPE_MASK as u128) << Self::TAG_SHIFT
            | (random & Self::RANDOM_MASK)
    }

    #[inline]
    pub fn from_u128(v: u128) -> Option<Self> {
        NonZeroU128::new(v).map(Self)
    }

    #[inline]
    pub fn as_u128(self) -> u128 {
        self.0.get()
    }

    /// Milliseconds since [`SVID_EPOCH`].
    #[inline]
    pub fn millis(self) -> u64 {
        (self.as_u128() >> Self::MILLIS_SHIFT) as u64 & Self::MILLIS_MASK
    }

    /// Milliseconds since the Unix epoch.
    #[inline]
    pub fn unix_millis(self) -> i64 {
        SVID_EPOCH * 1000 + self.millis() as i64
    }

    #[inline]
    pub fn tag(self) -> u8 {
        (self.as_u128() >> Self::TAG_SHIFT) as u8 & IDTYPE_MASK as u8
    }

    #[inline]
    pub fn random_bits(self) -> u128 {
        self.as_u128() & Self::RANDOM_MASK
    }

    /// 32 lowercase hex chars into `buf` (W3C trace-id), no allocation.
    #[inline]
    pub fn encode_hex_into(self, buf: &mut [u8; 32]) -> &str {
        hex_into(self.as_u128(), buf)
    }

    /// 32 lowercase hex chars (W3C trace-id).
    pub fn to_hex(self) -> String {
        self.encode_hex_into(&mut [0u8; 32]).to_owned()
    }

    /// Alias of [`to_hex`](Self::to_hex).
    #[inline]
    pub fn to_hex32(self) -> String {
        self.to_hex()
    }

    /// Parse exactly 32 lowercase hex chars; rejects all-zero.
    pub fn from_hex(s: &str) -> Result<Self, String> {
        Self::from_u128(decode_hex_exact(s, Self::HEX_LEN)?)
            .ok_or_else(|| "invalid Svid128: all-zero trace-id".to_string())
    }

    /// Full W3C `traceparent` (version 00) into `buf`:
    /// `00-<trace-id>-<span-id>-<flags>`, e.g. flags `0x01` = sampled.
    /// `span_id` is a 64-bit SVID; it must be non-zero (any ID minted after
    /// `SVID_EPOCH` + 1 s is).
    pub fn encode_traceparent_into(self, span_id: i64, flags: u8, buf: &mut [u8; 55]) -> &str {
        debug_assert!(span_id != 0, "W3C span-id must not be all-zero");
        buf[..3].copy_from_slice(b"00-");
        hex_into::<32>(self.as_u128(), (&mut buf[3..35]).try_into().unwrap());
        buf[35] = b'-';
        encode_hex16_into(span_id, (&mut buf[36..52]).try_into().unwrap());
        buf[52] = b'-';
        hex_into::<2>(flags as u128, (&mut buf[53..55]).try_into().unwrap());
        // SAFETY: all 55 bytes were written above from ASCII literals/tables.
        unsafe { crate::encoding::ascii(buf) }
    }
}

impl std::fmt::Display for Svid128 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.encode_hex_into(&mut [0u8; 32]))
    }
}

impl std::str::FromStr for Svid128 {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

#[cfg(feature = "http")]
impl From<Svid128> for http::HeaderValue {
    fn from(id: Svid128) -> Self {
        http::HeaderValue::from_bytes(id.encode_hex_into(&mut [0u8; 32]).as_bytes())
            .expect("hex is visible ASCII")
    }
}

#[cfg(feature = "http")]
impl TryFrom<&http::HeaderValue> for Svid128 {
    type Error = String;
    fn try_from(h: &http::HeaderValue) -> Result<Self, Self::Error> {
        Self::from_hex(h.to_str().map_err(|e| e.to_string())?)
    }
}

fn now_millis() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is before UNIX epoch")
        .as_millis() as i64;
    #[cfg(target_arch = "wasm32")]
    let now = js_sys::Date::now() as i64;
    (now - SVID_EPOCH * 1000).max(0) as u64
}
