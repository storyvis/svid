//! Bit-level helpers for the SVID layout.
//!
//! 64-bit layout (default `bits-balanced` profile):
//!
//! ```text
//!  63 62                   34 33                            8 7  6     0
//! ┌──┬──────────────────────┬───────────────────────────────┬──┬────────┐
//! │S0│    TIMESTAMP (T)     │         RANDOM (M)            │W │  TAG  │
//! │1b│     29 bits          │         26 bits               │1b│ 7 bits│
//! └──┴──────────────────────┴───────────────────────────────┴──┴────────┘
//!         T + M = 55 (varies by compile-time profile)
//! ```
//!
//! `tag` and `source` sit at fixed LSB positions across all profiles, so
//! downstream raw bit-ops like `id & 0x7F` stay stable when the bit budget
//! is reallocated. Timestamp is at the top (just below the sign bit) so
//! i64 ordering matches chronological order, as in ULID / Snowflake / UUIDv7.
//!
//! Profile selection (Cargo features):
//! - `bits-long-life`: T=31 (68 yr), M=24
//! - `bits-balanced` (default): T=29 (17 yr), M=26
//! - `bits-high-rand`: T=28 (8.5 yr), M=27

pub const SVID_EPOCH: i64 = 1767225600;

// --- Bit-layout profile selection (compile-time) ---
#[cfg(not(any(feature = "bits-long-life", feature = "bits-balanced", feature = "bits-high-rand")))]
compile_error!(
    "svid: enable exactly one bit-layout feature: bits-long-life | bits-balanced | bits-high-rand"
);

#[cfg(any(
    all(feature = "bits-long-life", feature = "bits-balanced"),
    all(feature = "bits-long-life", feature = "bits-high-rand"),
    all(feature = "bits-balanced", feature = "bits-high-rand"),
))]
compile_error!("svid: exactly one bit-layout feature may be enabled at a time");

#[cfg(feature = "bits-long-life")]
const _PROFILE: (u8, u8) = (31, 24);
#[cfg(feature = "bits-balanced")]
const _PROFILE: (u8, u8) = (29, 26);
#[cfg(feature = "bits-high-rand")]
const _PROFILE: (u8, u8) = (28, 27);

pub const TIMESTAMP_BITS: u8 = _PROFILE.0;
pub const RANDOM_BITS: u8 = _PROFILE.1;
pub const SOURCE_BITS: u8 = 1;
pub const IDTYPE_BITS: u8 = 7;

// 1 (sign) + TS + RAND + 1 (src) + 7 (tag) must total 64.
const _: () = assert!(
    1 + TIMESTAMP_BITS as u32 + RANDOM_BITS as u32 + SOURCE_BITS as u32 + IDTYPE_BITS as u32 == 64,
    "svid: bit-layout profile must sum to 64 bits"
);

pub const IDTYPE_MASK: i64 = (1 << IDTYPE_BITS) - 1;
pub const RANDOM_MASK: i64 = (1 << RANDOM_BITS) - 1;
pub const TIMESTAMP_MASK: i64 = (1 << TIMESTAMP_BITS) - 1;

/// Reserved tag value for untyped / random IDs minted via
/// [`SvidGenerator::generate_random`]. Lets `svid` stand in for nanoid /
/// uuidv4 when no domain enum is involved. User-defined `#[derive(Svid)]`
/// enums must not use this value as a discriminant; the derive macro
/// enforces this at compile time.
pub const RANDOM_ID_TAG: u8 = 127;

// Field order [sign][ts][rand][src][tag]: tag is LSB, ts is at the top.
pub const IDTYPE_SHIFT: u8 = 0;
pub const SOURCE_SHIFT: u8 = IDTYPE_BITS;
pub const RANDOM_SHIFT: u8 = SOURCE_SHIFT + SOURCE_BITS;
pub const TIMESTAMP_SHIFT: u8 = RANDOM_SHIFT + RANDOM_BITS;

/// Length of the fixed-width human-readable string format (`to_str()` / `from_str_id`).
/// Distinguishes from variable-length `to_base58()` via length check.
pub const HUMAN_READABLE_LEN: usize = 11;

/// Extension trait on `i64` exposing SVID bit-fields.
pub trait SvidExt {
    fn tag(&self) -> u8;
    fn timestamp_bits(&self) -> u32;
    fn is_client(&self) -> bool;
    fn random_bits(&self) -> u32;
    fn unix_timestamp(&self) -> i64;
}

impl SvidExt for i64 {
    #[inline]
    fn tag(&self) -> u8 {
        ((*self >> IDTYPE_SHIFT) & IDTYPE_MASK) as u8
    }
    #[inline]
    fn timestamp_bits(&self) -> u32 {
        ((*self >> TIMESTAMP_SHIFT) & TIMESTAMP_MASK) as u32
    }
    #[inline]
    fn is_client(&self) -> bool {
        ((*self >> SOURCE_SHIFT) & 1) == 1
    }
    #[inline]
    fn random_bits(&self) -> u32 {
        ((*self >> RANDOM_SHIFT) & RANDOM_MASK) as u32
    }
    #[inline]
    fn unix_timestamp(&self) -> i64 {
        SVID_EPOCH + self.timestamp_bits() as i64
    }
}

/// Pack the four SVID fields into a single `i64`.
#[inline]
pub fn encode_svid(timestamp: u32, is_client: bool, tag: u8, random: u32) -> i64 {
    ((timestamp as i64 & TIMESTAMP_MASK) << TIMESTAMP_SHIFT)
        | ((random as i64 & RANDOM_MASK) << RANDOM_SHIFT)
        | ((if is_client { 1i64 } else { 0i64 }) << SOURCE_SHIFT)
        | ((tag as i64 & IDTYPE_MASK) << IDTYPE_SHIFT)
}

/// Encode an `i64` SVID as a fixed-width 11-char string.
///
/// Zero-padded base58 of the 8 big-endian bytes. Always returns exactly
/// `HUMAN_READABLE_LEN` characters. The padding character is `'1'`
/// (the base58 representation of zero).
pub fn id_to_human_readable(id: i64) -> String {
    let s = bs58::encode(id.to_be_bytes()).into_string();
    debug_assert!(s.len() <= HUMAN_READABLE_LEN);
    if s.len() == HUMAN_READABLE_LEN {
        s
    } else {
        let pad = HUMAN_READABLE_LEN - s.len();
        let mut out = "1".repeat(pad);
        out.push_str(&s);
        out
    }
}

/// Decode a base58 string into an `i64` SVID.
///
/// Accepts variable-length base58 (≤ 8 decoded bytes). Used by every
/// `bs58 → i64` path in the crate so that error messages stay consistent.
///
/// Tolerates excess leading zero bytes that arise from fixed-width
/// padding in [`id_to_human_readable`] — when the i64's bs58 form is
/// shorter than `HUMAN_READABLE_LEN`, padding `'1'` chars get decoded as
/// leading zero bytes, which we strip back here.
pub fn decode_i64_base58(s: &str) -> Result<i64, String> {
    let bytes = bs58::decode(s).into_vec().map_err(|e| e.to_string())?;
    let trimmed: &[u8] = if bytes.len() > 8 {
        let excess = bytes.len() - 8;
        if bytes[..excess].iter().all(|&b| b == 0) {
            &bytes[excess..]
        } else {
            return Err(format!(
                "invalid base58 SVID: decoded {} bytes, expected <= 8",
                bytes.len()
            ));
        }
    } else {
        &bytes
    };
    let mut arr = [0u8; 8];
    arr[8 - trimmed.len()..].copy_from_slice(trimmed);
    let id = i64::from_be_bytes(arr);
    if id < 0 {
        return Err("invalid SVID: sign bit (bit 63) must be 0".to_string());
    }
    Ok(id)
}

/// Decode a human-readable SVID string back to `i64`.
pub fn human_readable_to_id(s: &str) -> Result<i64, String> {
    decode_i64_base58(s)
}

/// Decode a human-readable SVID and verify it carries the expected tag.
///
/// Enforces the fixed `HUMAN_READABLE_LEN`-character format. Use
/// [`decode_i64_base58`] for variable-length base58 inputs.
pub fn human_readable_to_id_expecting(s: &str, expected_tag: u8) -> Result<i64, String> {
    if s.len() != HUMAN_READABLE_LEN {
        return Err(format!(
            "Invalid human-readable SVID: expected {} chars, got {}",
            HUMAN_READABLE_LEN,
            s.len()
        ));
    }
    let id = human_readable_to_id(s)?;
    let got = id.tag();
    if got != expected_tag {
        return Err(format!(
            "Invalid SVID tag: expected {}, got {}",
            expected_tag, got
        ));
    }
    Ok(id)
}
