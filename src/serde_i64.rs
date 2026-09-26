//! Serde `with`-helpers that keep a raw `i64` svid on the Rust side but
//! serialize it as its base58 human-readable string on the wire.
//!
//! JS numbers are IEEE-754 doubles, so any svid above `2^53` silently corrupts
//! when a raw `i64` reaches JSON as a number. Use these on **polymorphic** id
//! fields — where no single [`SvidKind`](crate::SvidKind) newtype fits (e.g. an
//! `owner_id` that may reference a user, account, or group) — to emit/parse the
//! base58 form without changing the Rust type. For a single-tag field, prefer a
//! svid newtype (which also validates the tag on deserialize).
//!
//! Requires the `serde` feature.
//!
//! ```rust,ignore
//! #[serde(with = "svid::serde_i64")]
//! pub owner_id: i64,
//!
//! #[serde(with = "svid::serde_i64::option", default)]
//! pub parent_id: Option<i64>,
//! ```

use serde::{Deserialize, Deserializer, Serializer};

/// Serialize an `i64` svid as its base58 human-readable string.
pub fn serialize<S>(id: &i64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&crate::id_to_human_readable(*id))
}

/// Deserialize a base58 human-readable svid string back to `i64`.
pub fn deserialize<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    crate::decode_i64_base58(&s).map_err(serde::de::Error::custom)
}

/// `#[serde(with = "svid::serde_i64::option")]` for `Option<i64>` svid fields.
pub mod option {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(id: &Option<i64>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match id {
            Some(v) => serializer.serialize_some(&crate::id_to_human_readable(*v)),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt = Option::<String>::deserialize(deserializer)?;
        match opt {
            Some(s) => crate::decode_i64_base58(&s)
                .map(Some)
                .map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Poly {
        #[serde(with = "super")]
        id: i64,
        #[serde(with = "super::option", default)]
        opt: Option<i64>,
    }

    #[test]
    fn serializes_as_string_and_roundtrips() {
        let v = Poly { id: 342382391799224841, opt: Some(123456789012345678) };
        let json = serde_json::to_value(&v).unwrap();
        assert!(json["id"].is_string(), "id must be a JSON string, got {}", json["id"]);
        assert!(json["opt"].is_string(), "opt must be a JSON string, got {}", json["opt"]);
        let back: Poly = serde_json::from_value(json).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn value_above_2_pow_53_survives_intact() {
        // The whole point: an i64 above JS's 2^53 safe-integer limit must not
        // lose precision. As a number it would corrupt; as a string it is exact.
        let big: i64 = (1i64 << 53) + 12345;
        let v = Poly { id: big, opt: None };
        let s = serde_json::to_string(&v).unwrap();
        assert!(
            s.contains(&format!("\"{}\"", crate::id_to_human_readable(big))),
            "expected base58 string on the wire, got {s}"
        );
        assert!(!s.contains(&big.to_string()), "raw i64 leaked into JSON: {s}");
        let back: Poly = serde_json::from_str(&s).unwrap();
        assert_eq!(back.id, big);
    }

    #[test]
    fn option_none_is_null() {
        let v = Poly { id: 1, opt: None };
        let json = serde_json::to_value(&v).unwrap();
        assert!(json["opt"].is_null());
    }
}
