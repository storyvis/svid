//! SVID64 wire compatibility with 0.5.4: vectors produced by svid 0.5.4
//! `encode_svid` / `id_to_human_readable` for each profile.
use svid::{
    DecomposedSvid, SvidExt, encode_svid, human_readable_to_id_expecting, id_to_human_readable,
};

type Vector = (u32, bool, u8, u32, i64, &'static str);

#[cfg(feature = "bits-long-life")]
const VECTORS: [Vector; 5] = [
    (0, false, 0, 0x0, 0, "11111111111"),
    (1, true, 1, 0x1, 4294967681, "111117YXqFv"),
    (
        12345678,
        false,
        42,
        0xabcdef,
        53024286139346730,
        "1893o5qzoHj",
    ),
    (
        268435455,
        true,
        126,
        0xffffff,
        1152921504606846974,
        "3gDmDv6tjHF",
    ),
    (
        100000000,
        false,
        127,
        0x5,
        429496729600001407,
        "1zpnqnjJF8A",
    ),
];
#[cfg(feature = "bits-balanced")]
const VECTORS: [Vector; 5] = [
    (0, false, 0, 0x0, 0, "11111111111"),
    (1, true, 1, 0x1, 17179869569, "11111TB8Kgk"),
    (
        12345678,
        false,
        42,
        0xabcdef,
        212097135910186794,
        "1VZCB7Cs2pZ",
    ),
    (
        268435455,
        true,
        126,
        0xffffff,
        4611686005542486014,
        "Bht3tKmyRhF",
    ),
    (
        100000000,
        false,
        127,
        0x5,
        1717986918400001407,
        "4zJAN9vBxEr",
    ),
];
#[cfg(feature = "bits-high-rand")]
const VECTORS: [Vector; 5] = [
    (0, false, 0, 0x0, 0, "11111111111"),
    (1, true, 1, 0x1, 34359738753, "11111uMFeFr"),
    (
        12345678,
        false,
        42,
        0xabcdef,
        424194268937973546,
        "1z7PM9221Xf",
    ),
    (
        268435455,
        true,
        126,
        0xffffff,
        9223372006790004734,
        "NQm6mY1R2FF",
    ),
    (
        100000000,
        false,
        127,
        0x5,
        3435973836800001407,
        "8ybKjJqNu4S",
    ),
];

#[test]
fn svid64_matches_054() {
    for (ts, client, tag, random, id, text) in VECTORS {
        assert_eq!(encode_svid(ts, client, tag, random), id);
        assert_eq!(id_to_human_readable(id), text);
        assert_eq!(human_readable_to_id_expecting(text, tag), Ok(id));
        assert_eq!(
            (
                id.timestamp_bits(),
                id.is_client(),
                id.tag(),
                id.random_bits()
            ),
            (ts, client, tag, random)
        );
        assert_eq!(DecomposedSvid::from_i64(id).to_i64(), id);
    }
}

#[cfg(feature = "bits-balanced")]
#[test]
fn default_profile_is_054_default() {
    assert_eq!(
        (svid::TIMESTAMP_BITS, svid::RANDOM_BITS, svid::IDTYPE_BITS),
        (29, 26, 7)
    );
    assert_eq!(svid::RANDOM_ID_TAG, 127);
}
