//! Typed SVID64 and SVID128 entity identities for native Rust and WASM.
//!
//! SVID64 defaults to `[sign:1][seconds since 2026:29][random:26][source:1][type:7]`
//! (byte-compatible with 0.5.x; tag 127 is reserved).
//! SVID128 is a UUIDv8: `[unix ms:48][ver:4][random:12][var:2][random:49][source:1][type:12]`
//! (tag 4095 is reserved).
//! Entity IDs are time-prefixed, with random ordering within a timestamp bucket.
//! [`Sequencer`] provides process-local monotonic SVID64 generation only.
//!
//! [`TraceId128`] and [`SpanId64`] are separate W3C tracing types. Entity IDs
//! must not be used directly as trace IDs: their low metadata bits are not random.

pub mod encoding;
pub mod entity128;
pub mod generator;
pub mod monotonic;
pub mod svid128;
pub mod trace;
pub mod type_bits;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

/// Serde `with`-helpers to (de)serialize a raw `i64` svid as a base58 string
/// (JS-precision-safe). See [`serde_i64`]. Requires the `serde` feature.
#[cfg(feature = "serde")]
pub mod serde_i64;

pub use encoding::{encode_hex16_into, encode_str_into, hex16_to_id, id_to_hex16};
pub use generator::{GenerateId, IdGenerator, SvidKind};
pub use monotonic::Sequencer;
pub use svid128::Svid128;
pub use trace::{SpanId64, TraceContext, TraceId128};
pub use type_bits::{
    HUMAN_READABLE_LEN, IDTYPE_BITS, IDTYPE_MASK, IDTYPE_SHIFT, RANDOM_BITS, RANDOM_ID_TAG,
    RANDOM_ID_TAG128, RANDOM_MASK, RANDOM_SHIFT, SOURCE_BITS, SOURCE_SHIFT, SVID_EPOCH, SvidExt,
    TAG128_BITS, TAG128_MASK, TIMESTAMP_BITS, TIMESTAMP_MASK, TIMESTAMP_SHIFT, decode_i64_base58,
    encode_svid, human_readable_to_id, human_readable_to_id_expecting, id_to_human_readable,
    try_encode_svid,
};

pub use svid_macros::{Svid, SvidDomain, bridge};

// Re-exports so derive-generated code can reach helpers via ::svid::...
#[doc(hidden)]
pub use bs58;

#[cfg(feature = "strum")]
pub use strum;

#[cfg(feature = "http")]
pub use http;

/// Emits `From<$t> for http::HeaderValue` and `TryFrom<&HeaderValue> for $t`
/// for derive-generated ID types. Gated on *this* crate's `http` feature so
/// downstream crates need not mirror it.
#[cfg(feature = "http")]
#[doc(hidden)]
#[macro_export]
macro_rules! __svid_impl_http_width {
    ($t:ty, $len:expr) => {
        impl ::core::convert::From<$t> for $crate::http::HeaderValue {
            fn from(id: $t) -> Self {
                let mut buf = [0u8; $len];
                $crate::http::HeaderValue::from_bytes(id.encode_into(&mut buf).as_bytes())
                    .expect("base58 is visible ASCII")
            }
        }

        impl ::core::convert::TryFrom<&$crate::http::HeaderValue> for $t {
            type Error = ::std::string::String;
            fn try_from(
                h: &$crate::http::HeaderValue,
            ) -> ::core::result::Result<Self, Self::Error> {
                h.to_str()
                    .map_err(|e| ::std::string::ToString::to_string(&e))?
                    .parse()
            }
        }
    };
}

#[cfg(not(feature = "http"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __svid_impl_http_width {
    ($t:ty, $len:expr) => {};
}

/// Decomposed components of an SVID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecomposedSvid {
    pub timestamp: u32,
    pub is_client: bool,
    pub id_type: u8,
    pub random: u32,
}

impl DecomposedSvid {
    pub fn from_i64(id: i64) -> Self {
        Self {
            timestamp: id.timestamp_bits(),
            is_client: id.is_client(),
            id_type: id.tag(),
            random: id.random_bits(),
        }
    }

    pub fn to_i64(&self) -> i64 {
        encode_svid(self.timestamp, self.is_client, self.id_type, self.random)
    }

    pub fn unix_timestamp(&self) -> i64 {
        SVID_EPOCH + self.timestamp as i64
    }
}

pub struct SvidGenerator;

impl SvidGenerator {
    /// Generates a new SVID. Use `is_client = true` in WASM/client contexts.
    /// `id_type` is 0..=127; higher bits are masked off (0.5.x behaviour).
    ///
    /// Stateless by default; with the `monotonic` cargo feature this is
    /// [`generate_monotonic`](Self::generate_monotonic).
    #[inline]
    pub fn generate(id_type: u8, is_client: bool) -> i64 {
        #[cfg(feature = "monotonic")]
        return Self::generate_monotonic(id_type, is_client);
        #[cfg(not(feature = "monotonic"))]
        Self::generate_stateless(id_type, is_client)
    }

    /// Monotonic generation via the process-wide [`Sequencer`]: same format,
    /// zero in-process collisions, strictly increasing per tag, clock clamped.
    #[inline]
    pub fn generate_monotonic(id_type: u8, is_client: bool) -> i64 {
        Sequencer::global().generate(id_type, is_client)
    }

    #[cfg_attr(feature = "monotonic", allow(dead_code))]
    fn generate_stateless(id_type: u8, is_client: bool) -> i64 {
        debug_assert!(
            id_type as i64 <= IDTYPE_MASK,
            "id_type {id_type} exceeds 7-bit range (0..=127)"
        );
        let timestamp = Self::current_timestamp();
        let random = Self::random_field();
        encode_svid(timestamp, is_client, id_type, random)
    }

    /// Mints an untyped, random SVID — the drop-in replacement for nanoid /
    /// uuidv4 when no domain enum is needed. The tag field carries the
    /// reserved [`RANDOM_ID_TAG`] sentinel so callers can still recognize
    /// these IDs as untyped.
    pub fn generate_random(is_client: bool) -> i64 {
        Self::generate(RANDOM_ID_TAG, is_client)
    }

    /// Current timestamp field: seconds since [`SVID_EPOCH`], clamped at 0.
    pub fn current_timestamp() -> u32 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use std::time::{SystemTime, UNIX_EPOCH};
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is before UNIX epoch")
                .as_secs() as i64;
            u32::try_from((now - SVID_EPOCH).max(0)).expect("SVID64 timestamp exhausted")
        }
        #[cfg(target_arch = "wasm32")]
        {
            let now = (js_sys::Date::now() / 1000.0) as i64;
            u32::try_from((now - SVID_EPOCH).max(0)).expect("SVID64 timestamp exhausted")
        }
    }

    pub(crate) fn random_field() -> u32 {
        use rand::Rng;
        rand::rng().random::<u32>() & (RANDOM_MASK as u32)
    }
}

/// Mint a fresh typed ID. Works on both native and WASM targets.
///
/// The source bit is set automatically based on build target:
/// WASM → client-source, native → server-source.
///
/// ```ignore
/// let id: UserId = svid::mint::<UserIdMarker>();
/// ```
#[inline]
pub fn mint<M>() -> M::Id
where
    M: SvidKind,
    M::Id: From<M::Raw>,
{
    let id =
        <M::Raw as generator::SvidValue>::generate(M::TAG, cfg!(target_arch = "wasm32"), false);
    M::Id::from(id)
}

/// Untyped random ID (tag = [`RANDOM_ID_TAG`], 127). Use when no domain tag applies.
/// Works on both native and WASM targets.
#[inline]
pub fn random_id() -> i64 {
    SvidGenerator::generate_random(cfg!(target_arch = "wasm32"))
}
