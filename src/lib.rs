//! # SVID (Stateless Verifiable ID)
//!
//! A WASM-compatible, 64-bit ID generation module designed for unified
//! server and client (WASM) usage.
//!
//! ## Bit Layout
//!
//! ```text
//!  63 62              32 31 30      24 23                             0
//! ┌──┬──────────────────┬──┬──────────┬────────────────────────────────┐
//! │S0│    TIMESTAMP     │W │ ID TYPE  │            RANDOM              │
//! │1b│     31 bits      │1b│  7 bits  │            24 bits             │
//! └──┴──────────────────┴──┴──────────┴────────────────────────────────┘
//! ```
//!
//! - **Sign bit (63)**: Always 0 (ensures positive i64).
//! - **Timestamp (32-62)**: 31 bits, seconds since 2026-01-01 (~68 years).
//! - **WASM/Source bit (31)**: 1 = Client/WASM, 0 = Server.
//! - **ID Type (24-30)**: 7 bits (0-127) for domain-specific entity types.
//! - **Random (0-23)**: 24 bits cryptographic randomness (~16.7M unique per second).

pub mod generator;
pub mod type_bits;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub use generator::{IdGenerator, SvidKind};
pub use type_bits::{
    decode_i64_base58, encode_svid, id_to_human_readable, human_readable_to_id,
    human_readable_to_id_expecting, SvidExt, HUMAN_READABLE_LEN, IDTYPE_BITS, IDTYPE_MASK,
    IDTYPE_SHIFT, RANDOM_BITS, RANDOM_MASK, RANDOM_SHIFT, SOURCE_BITS, SOURCE_SHIFT, SVID_EPOCH,
    TIMESTAMP_BITS, TIMESTAMP_MASK, TIMESTAMP_SHIFT,
};

// Re-exports so generated macro output can address these via $crate::...
#[doc(hidden)]
pub use bs58;
#[doc(hidden)]
pub use paste;

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
    pub fn generate(id_type: u8, is_client: bool) -> i64 {
        debug_assert!(
            id_type <= 127,
            "id_type {} exceeds 7-bit range (0..=127)",
            id_type
        );
        let timestamp = Self::get_timestamp();
        let random = Self::get_random_24();
        encode_svid(timestamp, is_client, id_type, random)
    }

    fn get_timestamp() -> u32 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use std::time::{SystemTime, UNIX_EPOCH};
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is before UNIX epoch")
                .as_secs() as i64;
            (now - SVID_EPOCH).max(0) as u32
        }
        #[cfg(target_arch = "wasm32")]
        {
            let now = (js_sys::Date::now() / 1000.0) as i64;
            (now - SVID_EPOCH).max(0) as u32
        }
    }

    fn get_random_24() -> u32 {
        use rand::Rng;
        rand::thread_rng().gen::<u32>() & 0xFFFFFF
    }
}

// =============================================================================
// Helper Macros
//
// All macros assume an enum named `SvidTag` is in scope at the call site,
// `#[repr(u8)]` with one variant per ID newtype (variant name matches the
// newtype's name, e.g. `SvidTag::UserId = 1`).
//
// For the `serde` / `diesel` / `ts` feature blocks to compile, the *caller's*
// crate must enable a matching feature (typically by adding e.g.
// `diesel = ["svid/diesel"]` to its own `[features]` table) and depend
// directly on the corresponding crate.
// =============================================================================

/// Define a strongly-typed SVID.
///
/// Generates two things in one shot:
///
/// * **Runtime newtype** `pub struct $variant(pub i64)` with `to_base58` /
///   `from_base58` / `to_str` / `from_str_id` / `to_i64`, `Display`,
///   `FromStr`, and conditional serde/diesel/ts-rs impls. `from_base58` /
///   `from_str_id` verify the embedded tag matches `SvidTag::$variant`.
/// * **Compile-time marker** `pub struct ${variant}Marker` (zero-sized) plus
///   `impl SvidKind for ${variant}Marker { type Id = $variant; const TAG = SvidTag::$variant as u8; }`
///   used as a type parameter to `IdGenerator<K>` and `define_id_registry!`.
///
/// Requires `enum SvidTag` (with a variant named `$variant`) to be in scope at
/// the call site.
#[macro_export]
macro_rules! define_id {
    ($variant:ident) => {
        $crate::paste::paste! {
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
            #[cfg_attr(feature = "diesel", derive(::diesel::AsExpression, ::diesel::FromSqlRow))]
            #[cfg_attr(feature = "diesel", diesel(sql_type = ::diesel::sql_types::BigInt))]
            #[cfg_attr(feature = "ts", derive(::ts_rs::TS))]
            #[cfg_attr(feature = "ts", ts(export))]
            #[repr(transparent)]
            pub struct $variant(pub i64);

            impl ::std::convert::From<i64> for $variant {
                fn from(id: i64) -> Self { Self(id) }
            }

            impl $variant {
                /// Encode as variable-length base58 of the 8 big-endian bytes.
                pub fn to_base58(&self) -> String {
                    $crate::bs58::encode(self.0.to_be_bytes()).into_string()
                }

                /// Decode from base58, verifying the embedded tag matches.
                pub fn from_base58(s: &str) -> ::std::result::Result<Self, String> {
                    use $crate::SvidExt;
                    let id_val = $crate::decode_i64_base58(s)?;
                    let expected = SvidTag::$variant as u8;
                    let got = id_val.tag();
                    if got != expected {
                        return Err(format!(
                            "Invalid SVID tag: expected {} ({:?}), got {}",
                            expected, SvidTag::$variant, got
                        ));
                    }
                    Ok(Self(id_val))
                }

                /// Fixed-width 11-char string form (zero-padded base58).
                #[inline]
                pub fn to_str(&self) -> String {
                    $crate::id_to_human_readable(self.0)
                }

                /// Parse the fixed-width string form, verifying the embedded tag.
                #[inline]
                pub fn from_str_id(s: &str) -> ::std::result::Result<Self, String> {
                    $crate::human_readable_to_id_expecting(s, SvidTag::$variant as u8).map(Self)
                }

                #[inline]
                pub fn to_i64(&self) -> i64 { self.0 }
            }

            impl ::std::fmt::Display for $variant {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    write!(f, "{}", self.to_str())
                }
            }

            impl ::std::str::FromStr for $variant {
                type Err = String;
                fn from_str(s: &str) -> ::std::result::Result<Self, Self::Err> {
                    if s.len() == $crate::HUMAN_READABLE_LEN {
                        Self::from_str_id(s)
                    } else {
                        Self::from_base58(s)
                    }
                }
            }

            #[cfg(feature = "serde")]
            impl ::serde::Serialize for $variant {
                fn serialize<S>(&self, serializer: S) -> ::std::result::Result<S::Ok, S::Error>
                where S: ::serde::Serializer
                {
                    serializer.serialize_str(&self.to_str())
                }
            }

            #[cfg(feature = "serde")]
            impl<'de> ::serde::Deserialize<'de> for $variant {
                fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
                where D: ::serde::Deserializer<'de>
                {
                    let s = <String as ::serde::Deserialize>::deserialize(deserializer)?;
                    if s.len() == $crate::HUMAN_READABLE_LEN {
                        Self::from_str_id(&s).map_err(::serde::de::Error::custom)
                    } else {
                        Self::from_base58(&s).map_err(::serde::de::Error::custom)
                    }
                }
            }

            #[cfg(feature = "diesel")]
            impl ::diesel::serialize::ToSql<::diesel::sql_types::BigInt, ::diesel::pg::Pg> for $variant {
                fn to_sql<'b>(
                    &'b self,
                    out: &mut ::diesel::serialize::Output<'b, '_, ::diesel::pg::Pg>,
                ) -> ::diesel::serialize::Result {
                    use ::std::io::Write;
                    out.write_all(&self.0.to_be_bytes())?;
                    Ok(::diesel::serialize::IsNull::No)
                }
            }

            #[cfg(feature = "diesel")]
            impl ::diesel::deserialize::FromSql<::diesel::sql_types::BigInt, ::diesel::pg::Pg> for $variant {
                fn from_sql(
                    bytes: <::diesel::pg::Pg as ::diesel::backend::Backend>::RawValue<'_>,
                ) -> ::diesel::deserialize::Result<Self> {
                    let v = <i64 as ::diesel::deserialize::FromSql<
                        ::diesel::sql_types::BigInt,
                        ::diesel::pg::Pg,
                    >>::from_sql(bytes)?;
                    Ok(Self(v))
                }
            }

            // ---- compile-time marker ----
            #[derive(Debug, Clone, Copy, Default)]
            pub struct [<$variant Marker>];

            impl $crate::SvidKind for [<$variant Marker>] {
                type Id = $variant;
                const TAG: u8 = SvidTag::$variant as u8;
            }
        }
    };
}

/// Generate a registry struct holding one `IdGenerator<Marker>` per ID type.
///
/// Field names are snake_case of the newtype name (e.g. `user_id`).
#[macro_export]
macro_rules! define_id_registry {
    ($name:ident { $($variant:ident),* $(,)? }) => {
        $crate::paste::paste! {
            #[cfg(not(target_arch = "wasm32"))]
            pub struct $name {
                $(
                    pub [<$variant:snake>]: $crate::IdGenerator<[<$variant Marker>]>,
                )*
            }

            #[cfg(not(target_arch = "wasm32"))]
            impl $name {
                pub fn new(is_client: bool) -> Self {
                    Self {
                        $(
                            [<$variant:snake>]: $crate::IdGenerator::new(is_client),
                        )*
                    }
                }
            }
        }
    };
}

/// Generate a domain-scoped enum with variants wrapping newtype IDs.
///
/// Usage:
/// ```ignore
/// svid::define_domain_enum! {
///     /// Doc comment for the enum.
///     MyDomainId, "my-domain" {
///         Folder(FolderId),
///         Root(RootFolderId),
///     }
/// }
/// ```
#[macro_export]
macro_rules! define_domain_enum {
    (
        $(#[$meta:meta])*
        $enum_name:ident, $err_label:literal {
            $( $variant:ident($id_type:ident) ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[cfg_attr(feature = "diesel", derive(::diesel::AsExpression, ::diesel::FromSqlRow))]
        #[cfg_attr(feature = "diesel", diesel(sql_type = ::diesel::sql_types::BigInt))]
        #[cfg_attr(feature = "ts", derive(::ts_rs::TS))]
        #[cfg_attr(feature = "ts", ts(export))]
        pub enum $enum_name {
            $( $variant($id_type), )*
        }

        impl $enum_name {
            pub fn tag(&self) -> u8 {
                match self {
                    $( $enum_name::$variant(_) => SvidTag::$id_type as u8, )*
                }
            }

            pub fn to_i64(&self) -> i64 {
                match self {
                    $( $enum_name::$variant(id) => id.0, )*
                }
            }

            pub fn to_base58(&self) -> String {
                match self {
                    $( $enum_name::$variant(id) => id.to_base58(), )*
                }
            }

            pub fn from_i64(id: i64) -> ::std::result::Result<Self, String> {
                use $crate::SvidExt;
                let tag = id.tag();
                $(
                    if tag == SvidTag::$id_type as u8 {
                        return Ok($enum_name::$variant($id_type(id)));
                    }
                )*
                Err(format!(concat!("Invalid ", $err_label, " tag: {}"), tag))
            }

            pub fn from_base58(s: &str) -> ::std::result::Result<Self, String> {
                Self::from_i64($crate::decode_i64_base58(s)?)
            }

            #[inline]
            pub fn to_str(&self) -> String {
                $crate::id_to_human_readable(self.to_i64())
            }

            pub fn from_str_id(s: &str) -> ::std::result::Result<Self, String> {
                let id_val = $crate::human_readable_to_id(s)?;
                Self::from_i64(id_val)
            }
        }

        impl ::std::fmt::Display for $enum_name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, "{}", self.to_str())
            }
        }

        impl ::std::str::FromStr for $enum_name {
            type Err = String;
            fn from_str(s: &str) -> ::std::result::Result<Self, Self::Err> {
                if s.len() == $crate::HUMAN_READABLE_LEN {
                    Self::from_str_id(s)
                } else {
                    Self::from_base58(s)
                }
            }
        }

        #[cfg(feature = "serde")]
        impl ::serde::Serialize for $enum_name {
            fn serialize<S>(&self, serializer: S) -> ::std::result::Result<S::Ok, S::Error>
            where S: ::serde::Serializer
            {
                serializer.serialize_str(&self.to_str())
            }
        }

        #[cfg(feature = "serde")]
        impl<'de> ::serde::Deserialize<'de> for $enum_name {
            fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
            where D: ::serde::Deserializer<'de>
            {
                use ::std::str::FromStr;
                let s = <String as ::serde::Deserialize>::deserialize(deserializer)?;
                Self::from_str(&s).map_err(::serde::de::Error::custom)
            }
        }

        $(
            impl ::std::convert::From<$id_type> for $enum_name {
                fn from(id: $id_type) -> Self { $enum_name::$variant(id) }
            }

            impl ::std::convert::TryFrom<$enum_name> for $id_type {
                type Error = String;
                fn try_from(val: $enum_name) -> ::std::result::Result<Self, Self::Error> {
                    match val {
                        $enum_name::$variant(id) => Ok(id),
                        _ => Err(format!(
                            "Expected tag for {} ({}), got tag {}",
                            stringify!($id_type),
                            SvidTag::$id_type as u8,
                            val.tag(),
                        )),
                    }
                }
            }
        )*

        impl ::std::convert::TryFrom<i64> for $enum_name {
            type Error = String;
            fn try_from(id: i64) -> ::std::result::Result<Self, Self::Error> {
                Self::from_i64(id)
            }
        }

        impl ::std::convert::From<$enum_name> for i64 {
            fn from(val: $enum_name) -> Self { val.to_i64() }
        }

        #[cfg(feature = "diesel")]
        impl ::diesel::serialize::ToSql<::diesel::sql_types::BigInt, ::diesel::pg::Pg> for $enum_name {
            fn to_sql<'b>(
                &'b self,
                out: &mut ::diesel::serialize::Output<'b, '_, ::diesel::pg::Pg>,
            ) -> ::diesel::serialize::Result {
                use ::std::io::Write;
                out.write_all(&self.to_i64().to_be_bytes())?;
                Ok(::diesel::serialize::IsNull::No)
            }
        }

        #[cfg(feature = "diesel")]
        impl ::diesel::deserialize::FromSql<::diesel::sql_types::BigInt, ::diesel::pg::Pg> for $enum_name {
            fn from_sql(
                bytes: <::diesel::pg::Pg as ::diesel::backend::Backend>::RawValue<'_>,
            ) -> ::diesel::deserialize::Result<Self> {
                let v = <i64 as ::diesel::deserialize::FromSql<
                    ::diesel::sql_types::BigInt,
                    ::diesel::pg::Pg,
                >>::from_sql(bytes)?;
                <Self as ::std::convert::TryFrom<i64>>::try_from(v)
                    .map_err(|e: String| e.into())
            }
        }
    };
}

/// Bridge `impl From<Source> for Target` for two enum types whose variants
/// wrap the same set of newtype IDs.
///
/// Usage: `svid::define_enum_bridge!(MyDomainId -> UnifiedId { Folder(FolderId), Root(RootFolderId) });`
#[macro_export]
macro_rules! define_enum_bridge {
    ($src:ident -> $dst:ident { $( $variant:ident($id_type:ident) ),* $(,)? }) => {
        impl ::std::convert::From<$src> for $dst {
            fn from(val: $src) -> Self {
                match val {
                    $( $src::$variant(id) => <$dst as ::std::convert::From<$id_type>>::from(id), )*
                }
            }
        }
    };
}
