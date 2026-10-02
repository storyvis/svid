//! Allocation-free fixed-width encoders/decoders (base58, lowercase hex).
//!
//! The fixed 11-char base58 form is the zero-padded base58 of the `i64`
//! reinterpreted as `u64`, which is byte-identical to the `bs58`-based
//! [`id_to_human_readable`](crate::id_to_human_readable) output.

use crate::type_bits::HUMAN_READABLE_LEN;

const B58: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const B58_SQ: u64 = 58 * 58;
const B58_P5: u64 = 58 * 58 * 58 * 58 * 58;

static B58_PAIRS: [[u8; 2]; 58 * 58] = {
    let mut t = [[0u8; 2]; 58 * 58];
    let mut i = 0;
    while i < 58 * 58 {
        t[i] = [B58[i / 58], B58[i % 58]];
        i += 1;
    }
    t
};

const INVALID: u8 = 0xFF;

static B58_DECODE: [u8; 256] = {
    let mut t = [INVALID; 256];
    let mut i = 0;
    while i < 58 {
        t[B58[i] as usize] = i as u8;
        i += 1;
    }
    t
};

static HEX_PAIRS: [[u8; 2]; 256] = {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut t = [[0u8; 2]; 256];
    let mut i = 0;
    while i < 256 {
        t[i] = [HEX[i >> 4], HEX[i & 0xF]];
        i += 1;
    }
    t
};

static HEX_DECODE: [u8; 256] = {
    let mut t = [INVALID; 256];
    let mut i = 0;
    while i < 10 {
        t[b'0' as usize + i] = i as u8;
        i += 1;
    }
    let mut i = 0;
    while i < 6 {
        t[b'a' as usize + i] = 10 + i as u8;
        i += 1;
    }
    t
};

/// `from_utf8` validation costs more than the encoding itself here.
///
/// # Safety
/// Every byte of `buf` must be ASCII.
#[inline]
pub(crate) unsafe fn ascii(buf: &[u8]) -> &str {
    debug_assert!(buf.is_ascii());
    // SAFETY: ASCII is valid UTF-8 (caller guarantees ASCII).
    unsafe { std::str::from_utf8_unchecked(buf) }
}

/// Encode `id` as the fixed-width 11-char base58 string into `buf`, without
/// allocating. Output is byte-identical to [`id_to_human_readable`](crate::id_to_human_readable).
#[inline]
pub fn encode_str_into(id: i64, buf: &mut [u8; HUMAN_READABLE_LEN]) -> &str {
    // Split into 1 + 5 + 5 digits so the two 5-digit halves encode as
    // independent u32 chains instead of one serial chain of u64 divisions.
    let v = id as u64;
    let (hi, lo) = (v / B58_P5, (v % B58_P5) as u32);
    // u64::MAX < 58^11, so hi / 58^5 < 58.
    buf[0] = B58[(hi / B58_P5) as usize];
    put5(&mut buf[1..6], (hi % B58_P5) as u32);
    put5(&mut buf[6..11], lo);
    // SAFETY: every byte was just written from the ASCII base58 tables.
    unsafe { ascii(buf) }
}

#[inline(always)]
fn put5(out: &mut [u8], x: u32) {
    let sq = B58_SQ as u32;
    let y = x / sq;
    out[0] = B58[(y / sq) as usize];
    out[1..3].copy_from_slice(&B58_PAIRS[(y % sq) as usize]);
    out[3..5].copy_from_slice(&B58_PAIRS[(x % sq) as usize]);
}

/// Numeric base58 decode into `u64`. `None` on an invalid character or a
/// value ≥ 2^64 (callers fall back to `bs58` for the exact error message).
#[inline]
pub(crate) fn decode_base58_u64(s: &[u8]) -> Option<u64> {
    let mut v: u64 = 0;
    for &c in s {
        let d = B58_DECODE[c as usize];
        if d == INVALID {
            return None;
        }
        v = v.checked_mul(58)?.checked_add(d as u64)?;
    }
    Some(v)
}

#[inline]
pub(crate) fn hex_into<const N: usize>(mut v: u128, buf: &mut [u8; N]) -> &str {
    const { assert!(N & 1 == 0) };
    let mut i = N;
    while i > 0 {
        i -= 2;
        buf[i..i + 2].copy_from_slice(&HEX_PAIRS[(v & 0xFF) as usize]);
        v >>= 8;
    }
    // SAFETY: every byte was just written from the ASCII hex table.
    unsafe { ascii(buf) }
}

/// Strict lowercase-hex decode of exactly `n` digits (W3C trace-context
/// forbids uppercase).
#[inline]
pub(crate) fn decode_hex_exact(s: &str, n: usize) -> Result<u128, String> {
    if s.len() != n {
        return Err(format!(
            "invalid hex id: expected {} chars, got {}",
            n,
            s.len()
        ));
    }
    let mut v: u128 = 0;
    for (i, &c) in s.as_bytes().iter().enumerate() {
        let d = HEX_DECODE[c as usize];
        if d == INVALID {
            return Err(format!(
                "invalid hex id: non-lowercase-hex character at byte {}",
                i
            ));
        }
        v = (v << 4) | d as u128;
    }
    Ok(v)
}

/// Case-insensitive hex decode of exactly 32 digits from `s` (UUID text,
/// RFC 9562 §4).
#[inline]
pub(crate) fn decode_hex32_any_case(s: &[u8; 32]) -> Option<u128> {
    let mut v: u128 = 0;
    for &c in s {
        let d = HEX_DECODE[c.to_ascii_lowercase() as usize];
        if d == INVALID {
            return None;
        }
        v = (v << 4) | d as u128;
    }
    Some(v)
}

/// Encode `id` as 16 lowercase hex chars (big-endian) into `buf`, e.g. for a
/// W3C `traceparent` span-id. Any ID minted at least one second after
/// [`SVID_EPOCH`](crate::SVID_EPOCH) has a non-zero timestamp field, so the
/// result is never the all-zero span-id W3C forbids.
#[inline]
pub fn encode_hex16_into(id: i64, buf: &mut [u8; 16]) -> &str {
    hex_into(id as u64 as u128, buf)
}

/// Allocating convenience over [`encode_hex16_into`].
pub fn id_to_hex16(id: i64) -> String {
    encode_hex16_into(id, &mut [0u8; 16]).to_owned()
}

/// Parse 16 lowercase hex chars back into an SVID `i64`. Rejects the sign
/// bit, matching the base58 decoders.
pub fn hex16_to_id(s: &str) -> Result<i64, String> {
    let id = decode_hex_exact(s, 16)? as u64 as i64;
    if id < 0 {
        return Err("invalid SVID: sign bit (bit 63) must be 0".to_string());
    }
    Ok(id)
}
