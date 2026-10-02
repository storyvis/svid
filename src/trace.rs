//! W3C trace identity, separate from durable entity identity.
//! Layout: [milliseconds since 2026:41][type:12][random:75].
//! The rightmost 56 bits remain random. Zero is excluded by construction.
//! Trace metadata intentionally has different positions from entity metadata.

use std::num::NonZeroU128;

use crate::encoding::{decode_hex_exact, encode_hex16_into, hex_into};
use crate::type_bits::{IDTYPE_MASK, RANDOM_ID_TAG, SVID_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TraceId128(NonZeroU128);

impl TraceId128 {
    pub const MILLIS_BITS: u32 = 41;
    pub const TAG_BITS: u32 = 12;
    pub const RANDOM_BITS: u32 = 75;
    pub const TAG_SHIFT: u32 = Self::RANDOM_BITS;
    pub const MILLIS_SHIFT: u32 = Self::TAG_SHIFT + Self::TAG_BITS;
    pub const RANDOM_MASK: u128 = (1 << Self::RANDOM_BITS) - 1;
    pub const MILLIS_MASK: u64 = (1 << Self::MILLIS_BITS) - 1;
    /// Length of [`to_hex`](Self::to_hex) output.
    pub const HEX_LEN: usize = 32;

    /// Fresh ID with the given tag (0..=4095).
    pub fn generate(tag: u16) -> Self {
        use rand::Rng;
        assert!(tag <= IDTYPE_MASK as u16, "trace type exceeds 12 bits");
        let random = rand::rng().random::<u128>();
        let millis = now_millis();
        assert!(millis <= Self::MILLIS_MASK, "trace timestamp exhausted");
        let v = Self::pack(millis, tag, random);
        // Only reachable before SVID_EPOCH with tag 0 and 2^-75 luck.
        Self(NonZeroU128::new(v).unwrap_or(NonZeroU128::MIN))
    }

    /// Untyped trace ID carrying [`RANDOM_ID_TAG`].
    pub fn generate_random() -> Self {
        Self::generate(RANDOM_ID_TAG)
    }

    /// Pack explicit fields. `None` if every field is zero or any field exceeds its bit budget.
    pub fn from_parts(millis: u64, tag: u16, random: u128) -> Option<Self> {
        if millis > Self::MILLIS_MASK || tag > IDTYPE_MASK as u16 || random > Self::RANDOM_MASK {
            return None;
        }
        Self::from_u128(Self::pack(millis, tag, random))
    }

    #[inline]
    fn pack(millis: u64, tag: u16, random: u128) -> u128 {
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
    pub fn tag(self) -> u16 {
        (self.as_u128() >> Self::TAG_SHIFT) as u16 & IDTYPE_MASK as u16
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
            .ok_or_else(|| "invalid TraceId128: all-zero trace-id".to_string())
    }

    /// Full W3C `traceparent` (version 00) into `buf`:
    /// `00-<trace-id>-<span-id>-<flags>`, e.g. flags `0x01` = sampled.
    /// `span_id` is a 64-bit SVID; it must be non-zero (any ID minted after
    /// `SVID_EPOCH` + 1 s is).
    pub fn encode_traceparent_into(self, span_id: SpanId64, flags: u8, buf: &mut [u8; 55]) -> &str {
        buf[..3].copy_from_slice(b"00-");
        hex_into::<32>(self.as_u128(), (&mut buf[3..35]).try_into().unwrap());
        buf[35] = b'-';
        encode_hex16_into(
            span_id.0.get() as i64,
            (&mut buf[36..52]).try_into().unwrap(),
        );
        buf[52] = b'-';
        hex_into::<2>(flags as u128, (&mut buf[53..55]).try_into().unwrap());
        // SAFETY: all 55 bytes were written above from ASCII literals/tables.
        unsafe { crate::encoding::ascii(buf) }
    }
}

impl std::fmt::Display for TraceId128 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.encode_hex_into(&mut [0u8; 32]))
    }
}

impl std::str::FromStr for TraceId128 {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

#[cfg(feature = "http")]
impl From<TraceId128> for http::HeaderValue {
    fn from(id: TraceId128) -> Self {
        http::HeaderValue::from_bytes(id.encode_hex_into(&mut [0u8; 32]).as_bytes())
            .expect("hex is visible ASCII")
    }
}

#[cfg(feature = "http")]
impl TryFrom<&http::HeaderValue> for TraceId128 {
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

/// A W3C span identifier: all 64 bits available, zero excluded by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpanId64(std::num::NonZeroU64);
impl SpanId64 {
    pub fn from_u64(value: u64) -> Option<Self> {
        std::num::NonZeroU64::new(value).map(Self)
    }
    pub fn as_u64(self) -> u64 {
        self.0.get()
    }
    pub fn generate() -> Self {
        use rand::Rng;
        loop {
            if let Some(id) = Self::from_u64(rand::rng().random()) {
                return id;
            }
        }
    }
    pub fn from_hex(value: &str) -> Result<Self, String> {
        Self::from_u64(decode_hex_exact(value, 16)? as u64).ok_or_else(|| "all-zero span ID".into())
    }
    pub fn encode_hex_into(self, buf: &mut [u8; 16]) -> &str {
        encode_hex16_into(self.0.get() as i64, buf)
    }
}
