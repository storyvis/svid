//! Durable entity identity: [Unix milliseconds:48][random:67][source:1][type:12].
//! Metadata occupies the same low bits as SVID64. This is not a trace ID or UUIDv7.
use crate::{IDTYPE_MASK, RANDOM_ID_TAG, SOURCE_SHIFT};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Svid128(#[cfg_attr(feature = "ts", ts(type = "string"))] u128);

impl Svid128 {
    pub const NIL: Self = Self(0);
    pub const MILLIS_BITS: u32 = 48;
    pub const RANDOM_BITS: u32 = 67;
    pub const RANDOM_SHIFT: u32 = crate::RANDOM_SHIFT as u32;
    pub const MILLIS_SHIFT: u32 = 80;
    pub const MILLIS_MASK: u64 = (1u64 << Self::MILLIS_BITS) - 1;
    pub const RANDOM_MASK: u128 = (1u128 << Self::RANDOM_BITS) - 1;
    pub const TEXT_LEN: usize = 36;
    pub const HEX_LEN: usize = 32;

    pub fn generate(tag: u16) -> Self {
        Self::generate_with_source(tag, cfg!(target_arch = "wasm32"))
    }
    pub fn generate_random() -> Self {
        Self::generate(RANDOM_ID_TAG)
    }
    pub fn generate_with_source(tag: u16, is_client: bool) -> Self {
        use rand::Rng;
        let random = rand::rng().random::<u128>() & Self::RANDOM_MASK;
        Self::from_parts(now_millis(), is_client, tag, random)
            .expect("SVID128 timestamp or type out of range")
    }
    /// Checked packing; rejects every oversized field, including in release builds.
    pub const fn from_parts(millis: u64, is_client: bool, tag: u16, random: u128) -> Option<Self> {
        if millis > Self::MILLIS_MASK || tag > IDTYPE_MASK as u16 || random > Self::RANDOM_MASK {
            return None;
        }
        Some(Self(
            ((millis as u128) << Self::MILLIS_SHIFT)
                | (random << Self::RANDOM_SHIFT)
                | ((is_client as u128) << SOURCE_SHIFT)
                | tag as u128,
        ))
    }
    pub const fn from_u128(value: u128) -> Self {
        Self(value)
    }
    pub const fn as_u128(self) -> u128 {
        self.0
    }
    pub const fn from_be_bytes(value: [u8; 16]) -> Self {
        Self(u128::from_be_bytes(value))
    }
    pub const fn to_be_bytes(self) -> [u8; 16] {
        self.0.to_be_bytes()
    }
    pub const fn is_nil(self) -> bool {
        self.0 == 0
    }
    pub const fn tag(self) -> u16 {
        (self.0 & IDTYPE_MASK as u128) as u16
    }
    pub const fn is_client(self) -> bool {
        self.0 & (1 << SOURCE_SHIFT) != 0
    }
    pub const fn millis(self) -> u64 {
        (self.0 >> Self::MILLIS_SHIFT) as u64
    }
    pub const fn unix_millis(self) -> i64 {
        self.millis() as i64
    }
    pub const fn random_bits(self) -> u128 {
        (self.0 >> Self::RANDOM_SHIFT) & Self::RANDOM_MASK
    }
    pub fn encode_hex_into(self, buf: &mut [u8; 32]) -> &str {
        crate::encoding::hex_into(self.0, buf)
    }
    pub fn to_hex(self) -> String {
        self.encode_hex_into(&mut [0; 32]).to_owned()
    }
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
    pub fn from_hex(value: &str) -> Result<Self, String> {
        crate::encoding::decode_hex_exact(value, 32).map(Self)
    }
}
impl std::fmt::Display for Svid128 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.encode_into(&mut [0; 36]))
    }
}
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
        Self::from_hex(std::str::from_utf8(&hex).map_err(|e| e.to_string())?)
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
        Ok(Self::from_be_bytes(bytes.as_bytes().try_into()?))
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
        Ok(Self::from_be_bytes(raw.try_into()?))
    }
    fn accepts(ty: &postgres_types::Type) -> bool {
        *ty == postgres_types::Type::UUID
    }
}
