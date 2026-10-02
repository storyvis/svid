//! Durable entity identity as an RFC 9562 UUIDv8:
//!
//! ```text
//! 127        80 79  76 75    64 63 62 61         13 12  11      0
//! ┌───────────┬──────┬────────┬─────┬─────────────┬───┬─────────┐
//! │ unix ms 48│ ver 8│ rand 12│ var │   rand 49   │src│ type 12 │
//! └───────────┴──────┴────────┴─────┴─────────────┴───┴─────────┘
//! ```
//!
//! 61 random bits per millisecond, time-ordered by the top 48 bits. The 12-bit
//! type and source bit sit at fixed LSB positions. This is not a trace ID:
//! use [`TraceId128`](crate::TraceId128) for W3C trace context.
use std::num::NonZeroU128;

use crate::type_bits::{RANDOM_ID_TAG128, TAG128_BITS, TAG128_MASK};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Svid128(#[cfg_attr(feature = "ts", ts(type = "string"))] NonZeroU128);

impl Svid128 {
    pub const MILLIS_BITS: u32 = 48;
    pub const MILLIS_SHIFT: u32 = 80;
    pub const MILLIS_MASK: u64 = (1u64 << Self::MILLIS_BITS) - 1;
    pub const TAG_BITS: u32 = TAG128_BITS;
    pub const SOURCE_SHIFT: u32 = TAG128_BITS;
    /// Total random bits ([`random_bits`](Self::random_bits) packs `rand_a:12 | rand_b:49`).
    pub const RANDOM_BITS: u32 = 61;
    pub const RANDOM_MASK: u64 = (1u64 << Self::RANDOM_BITS) - 1;
    const RAND_B_BITS: u32 = 49;
    const RAND_B_SHIFT: u32 = Self::SOURCE_SHIFT + 1;
    const RAND_B_MASK: u128 = (1u128 << Self::RAND_B_BITS) - 1;
    const RAND_A_SHIFT: u32 = 64;
    const RAND_A_MASK: u128 = 0xFFF;
    const VERSION_VARIANT_MASK: u128 = (0xF << 76) | (0b11 << 62);
    const VERSION_VARIANT: u128 = (0x8 << 76) | (0b10 << 62);
    pub const TEXT_LEN: usize = 36;
    pub const HEX_LEN: usize = 32;

    /// Fresh ID; the source bit is set on wasm32. Higher tag bits are masked off.
    pub fn generate(tag: u16) -> Self {
        Self::generate_with_source(tag, cfg!(target_arch = "wasm32"))
    }
    /// Untyped ID carrying [`RANDOM_ID_TAG128`].
    pub fn generate_random() -> Self {
        Self::generate(RANDOM_ID_TAG128)
    }
    pub fn generate_with_source(tag: u16, is_client: bool) -> Self {
        use rand::Rng;
        let random = rand::rng().random::<u64>() & Self::RANDOM_MASK;
        Self::pack(
            now_millis() & Self::MILLIS_MASK,
            is_client,
            tag & TAG128_MASK,
            random,
        )
    }
    /// Checked packing; `None` if any field exceeds its bit budget.
    pub const fn from_parts(millis: u64, is_client: bool, tag: u16, random: u64) -> Option<Self> {
        if millis > Self::MILLIS_MASK || tag > TAG128_MASK || random > Self::RANDOM_MASK {
            return None;
        }
        Some(Self::pack(millis, is_client, tag, random))
    }
    const fn pack(millis: u64, is_client: bool, tag: u16, random: u64) -> Self {
        let random = random as u128;
        let v = ((millis as u128) << Self::MILLIS_SHIFT)
            | Self::VERSION_VARIANT
            | ((random >> Self::RAND_B_BITS) << Self::RAND_A_SHIFT)
            | ((random & Self::RAND_B_MASK) << Self::RAND_B_SHIFT)
            | ((is_client as u128) << Self::SOURCE_SHIFT)
            | tag as u128;
        // SAFETY: the version/variant bits are non-zero.
        Self(unsafe { NonZeroU128::new_unchecked(v) })
    }
    /// `None` unless `value` carries the UUIDv8 version and RFC 9562 variant bits.
    pub const fn from_u128(value: u128) -> Option<Self> {
        if value & Self::VERSION_VARIANT_MASK != Self::VERSION_VARIANT {
            return None;
        }
        // SAFETY: the version/variant bits are non-zero.
        Some(Self(unsafe { NonZeroU128::new_unchecked(value) }))
    }
    pub const fn as_u128(self) -> u128 {
        self.0.get()
    }
    pub const fn from_be_bytes(value: [u8; 16]) -> Option<Self> {
        Self::from_u128(u128::from_be_bytes(value))
    }
    pub const fn to_be_bytes(self) -> [u8; 16] {
        self.as_u128().to_be_bytes()
    }
    pub const fn tag(self) -> u16 {
        (self.as_u128() & TAG128_MASK as u128) as u16
    }
    pub const fn is_client(self) -> bool {
        self.as_u128() & (1 << Self::SOURCE_SHIFT) != 0
    }
    pub const fn millis(self) -> u64 {
        (self.as_u128() >> Self::MILLIS_SHIFT) as u64
    }
    pub const fn unix_millis(self) -> i64 {
        self.millis() as i64
    }
    pub const fn random_bits(self) -> u64 {
        let v = self.as_u128();
        let a = (v >> Self::RAND_A_SHIFT) & Self::RAND_A_MASK;
        let b = (v >> Self::RAND_B_SHIFT) & Self::RAND_B_MASK;
        ((a << Self::RAND_B_BITS) | b) as u64
    }
    pub fn encode_hex_into(self, buf: &mut [u8; 32]) -> &str {
        crate::encoding::hex_into(self.as_u128(), buf)
    }
    pub fn to_hex(self) -> String {
        self.encode_hex_into(&mut [0; 32]).to_owned()
    }
    /// Canonical lowercase UUID text.
    pub fn encode_into(self, buf: &mut [u8; 36]) -> &str {
        let mut hex = [0; 32];
        self.encode_hex_into(&mut hex);
        let mut j = 0;
        for (i, out) in buf.iter_mut().enumerate() {
            if matches!(i, 8 | 13 | 18 | 23) {
                *out = b'-';
            } else {
                *out = hex[j];
                j += 1;
            }
        }
        // SAFETY: every byte comes from the ASCII hex table or a hyphen.
        unsafe { crate::encoding::ascii(buf) }
    }
    /// 32 hex digits, either case.
    pub fn from_hex(value: &str) -> Result<Self, String> {
        let hex: &[u8; 32] = value.as_bytes().try_into().map_err(|_| {
            format!(
                "invalid SVID128: expected 32 hex digits, got {}",
                value.len()
            )
        })?;
        Self::from_hex32(hex)
    }
    fn from_hex32(hex: &[u8; 32]) -> Result<Self, String> {
        let v = crate::encoding::decode_hex32_any_case(hex)
            .ok_or_else(|| "invalid SVID128: non-hex character".to_string())?;
        Self::from_u128(v)
            .ok_or_else(|| "invalid SVID128: not a UUIDv8 (version/variant bits)".into())
    }
}
impl std::fmt::Display for Svid128 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.encode_into(&mut [0; 36]))
    }
}
/// Accepts 32 hex digits or hyphenated UUID text, either case.
impl std::str::FromStr for Svid128 {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() == 32 {
            return Self::from_hex(s);
        }
        if s.len() != 36 {
            return Err("SVID128 requires 32 hex digits or canonical UUID text".into());
        }
        let mut hex = [0; 32];
        let mut j = 0;
        for (i, byte) in s.bytes().enumerate() {
            if matches!(i, 8 | 13 | 18 | 23) {
                if byte != b'-' {
                    return Err("invalid SVID128 UUID separator".into());
                }
            } else {
                hex[j] = byte;
                j += 1;
            }
        }
        Self::from_hex32(&hex)
    }
}
#[cfg(feature = "serde")]
impl serde::Serialize for Svid128 {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.encode_into(&mut [0; 36]))
    }
}
#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Svid128 {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl serde::de::Visitor<'_> for Visitor {
            type Value = Svid128;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a SVID128 UUID string")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Svid128, E> {
                v.parse().map_err(E::custom)
            }
        }
        d.deserialize_str(Visitor)
    }
}
#[cfg(feature = "diesel")]
impl diesel::serialize::ToSql<diesel::sql_types::Uuid, diesel::pg::Pg> for Svid128 {
    fn to_sql<'b>(
        &'b self,
        out: &mut diesel::serialize::Output<'b, '_, diesel::pg::Pg>,
    ) -> diesel::serialize::Result {
        use std::io::Write;
        out.write_all(&self.to_be_bytes())?;
        Ok(diesel::serialize::IsNull::No)
    }
}
#[cfg(feature = "diesel")]
impl diesel::deserialize::FromSql<diesel::sql_types::Uuid, diesel::pg::Pg> for Svid128 {
    fn from_sql(bytes: diesel::pg::PgValue<'_>) -> diesel::deserialize::Result<Self> {
        let raw: [u8; 16] = bytes.as_bytes().try_into()?;
        Ok(Self::from_be_bytes(raw).ok_or("not an SVID128 (UUIDv8) value")?)
    }
}
#[cfg(feature = "autosurgeon")]
impl autosurgeon::Reconcile for Svid128 {
    type Key<'a> = autosurgeon::reconcile::NoKey;
    fn reconcile<R: autosurgeon::Reconciler>(&self, mut r: R) -> Result<(), R::Error> {
        r.str(self.encode_into(&mut [0; 36]))
    }
}
#[cfg(feature = "autosurgeon")]
impl autosurgeon::Hydrate for Svid128 {
    fn hydrate_string(value: &str) -> Result<Self, autosurgeon::HydrateError> {
        value
            .parse()
            .map_err(|e: String| autosurgeon::HydrateError::unexpected("SVID128 UUID", e))
    }
}
crate::__svid_impl_http_width!(Svid128, 36);

fn now_millis() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock before Unix epoch")
            .as_millis()
            .try_into()
            .expect("clock overflow")
    }
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now().max(0.0) as u64
    }
}

#[cfg(feature = "postgres")]
impl postgres_types::ToSql for Svid128 {
    fn to_sql(
        &self,
        _: &postgres_types::Type,
        out: &mut bytes::BytesMut,
    ) -> Result<postgres_types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
        out.extend_from_slice(&self.to_be_bytes());
        Ok(postgres_types::IsNull::No)
    }
    fn accepts(ty: &postgres_types::Type) -> bool {
        *ty == postgres_types::Type::UUID
    }
    postgres_types::to_sql_checked!();
}
#[cfg(feature = "postgres")]
impl<'a> postgres_types::FromSql<'a> for Svid128 {
    fn from_sql(
        _: &postgres_types::Type,
        raw: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        let raw: [u8; 16] = raw.try_into()?;
        Ok(Self::from_be_bytes(raw).ok_or("not an SVID128 (UUIDv8) value")?)
    }
    fn accepts(ty: &postgres_types::Type) -> bool {
        *ty == postgres_types::Type::UUID
    }
}
