//! # SVID (Stateless Verifiable ID)
//!
//! A WASM-compatible, 64-bit ID generation module designed for unified
//! server and client (WASM) usage.
//!
//! ## Bit Layout (default `bits-balanced` profile)
//!
//! ```text
//!  63 62                   34 33                            8 7  6     0
//! ┌──┬──────────────────────┬───────────────────────────────┬──┬────────┐
//! │S0│    TIMESTAMP (T)     │         RANDOM (M)            │W │  TAG  │
//! │1b│     29 bits          │         26 bits               │1b│ 7 bits│
//! └──┴──────────────────────┴───────────────────────────────┴──┴────────┘
//! ```
//!
//! - **Sign bit (63)**: Always 0 (ensures positive i64).
//! - **Timestamp**: T bits, seconds since 2026-01-01. Sits at the top so
//!   i64 ordering matches chronological order (like ULID/Snowflake/UUIDv7).
//! - **Random**: M bits of cryptographic randomness. T + M = 55.
//! - **WASM/Source bit (7)**: 1 = Client/WASM, 0 = Server.
//! - **TAG (0-6)**: 7 bits (0-127) for domain-specific entity types.
//!   Always at the LSB so `id & 0x7F` extracts the tag in any profile.
//!
//! ## Profile selection (Cargo features)
//!
//! Enable exactly one:
//! - `bits-long-life`: T=31 (68 yr), M=24 — original timestamp range
//! - `bits-balanced` (default): T=29 (17 yr), M=26 — recommended
//! - `bits-high-rand`: T=28 (8.5 yr), M=27 — short-lived high-rate apps

pub mod generator;
pub mod type_bits;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub use generator::{GenerateId, IdGenerator, SvidKind};
pub use type_bits::{
    decode_i64_base58, encode_svid, human_readable_to_id, human_readable_to_id_expecting,
    id_to_human_readable, SvidExt, HUMAN_READABLE_LEN, IDTYPE_BITS, IDTYPE_MASK, IDTYPE_SHIFT,
    RANDOM_BITS, RANDOM_MASK, RANDOM_SHIFT, SOURCE_BITS, SOURCE_SHIFT, SVID_EPOCH, TIMESTAMP_BITS,
    TIMESTAMP_MASK, TIMESTAMP_SHIFT,
};

pub use svid_macros::{bridge, Svid, SvidDomain};

// Re-exports so derive-generated code can reach helpers via ::svid::...
#[doc(hidden)]
pub use bs58;

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
        let random = Self::get_random();
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

    fn get_random() -> u32 {
        use rand::Rng;
        rand::thread_rng().gen::<u32>() & (RANDOM_MASK as u32)
    }
}
