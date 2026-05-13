//! Bit-level helpers for the SVID layout.
//!
//! The 64-bit layout:
//!
//! ```text
//!  63 62              32 31 30      24 23                             0
//! ┌──┬──────────────────┬──┬──────────┬────────────────────────────────┐
//! │S0│    TIMESTAMP     │W │ ID TYPE  │            RANDOM              │
//! │1b│     31 bits      │1b│  7 bits  │            24 bits             │
//! └──┴──────────────────┴──┴──────────┴────────────────────────────────┘
//! ```

pub const SVID_EPOCH: i64 = 1767225600;

pub const TIMESTAMP_BITS: u8 = 31;
pub const SOURCE_BITS: u8 = 1;
pub const IDTYPE_BITS: u8 = 7;
pub const RANDOM_BITS: u8 = 24;

pub const RANDOM_MASK: i64 = (1 << RANDOM_BITS) - 1;
pub const IDTYPE_MASK: i64 = (1 << IDTYPE_BITS) - 1;
pub const TIMESTAMP_MASK: i64 = (1 << TIMESTAMP_BITS) - 1;

pub const RANDOM_SHIFT: u8 = 0;
pub const IDTYPE_SHIFT: u8 = 24;
pub const SOURCE_SHIFT: u8 = 31;
pub const TIMESTAMP_SHIFT: u8 = 32;

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
        (*self & RANDOM_MASK) as u32
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
        | ((if is_client { 1i64 } else { 0i64 }) << SOURCE_SHIFT)
        | ((tag as i64 & IDTYPE_MASK) << IDTYPE_SHIFT)
        | (random as i64 & RANDOM_MASK)
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
pub fn decode_i64_base58(s: &str) -> Result<i64, String> {
    let bytes = bs58::decode(s).into_vec().map_err(|e| e.to_string())?;
    if bytes.len() > 8 {
        return Err(format!(
            "invalid base58 SVID: decoded {} bytes, expected <= 8",
            bytes.len()
        ));
    }
    let mut arr = [0u8; 8];
    arr[8 - bytes.len()..].copy_from_slice(&bytes);
    Ok(i64::from_be_bytes(arr))
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
