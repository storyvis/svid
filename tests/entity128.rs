use svid::{Svid128, SvidExt};
#[derive(svid::Svid, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u16)]
#[svid(registry = Entities, bits = 128)]
pub enum Tags {
    UserId = 256,
    FileId = 4094,
}

#[test]
fn metadata_alignment_and_boundaries() {
    for tag in [0, 127, 128, 255, 256, 2048, 4094, 4095] {
        for client in [false, true] {
            let narrow = svid::encode_svid(17, client, tag, 123);
            let wide = Svid128::from_parts(17, client, tag, 123).unwrap();
            assert_eq!(narrow as u128 & 0x1fff, wide.as_u128() & 0x1fff);
            assert_eq!(narrow.tag(), wide.tag());
            assert_eq!(narrow.is_client(), wide.is_client());
            assert_eq!(wide.random_bits(), 123);
            assert_eq!(wide.millis(), 17);
        }
    }
    let max = Svid128::from_parts(Svid128::MILLIS_MASK, true, 4095, Svid128::RANDOM_MASK).unwrap();
    assert_eq!(max.as_u128(), u128::MAX);
    assert!(Svid128::from_parts(0, false, 4096, 0).is_none());
    assert!(Svid128::from_parts(Svid128::MILLIS_MASK + 1, false, 0, 0).is_none());
    assert!(Svid128::from_parts(0, false, 0, Svid128::RANDOM_MASK + 1).is_none());
    assert!(std::panic::catch_unwind(|| svid::encode_svid(0, false, 4096, 0)).is_err());
    assert!(
        std::panic::catch_unwind(|| svid::encode_svid(
            svid::TIMESTAMP_MASK as u32 + 1,
            false,
            1,
            0
        ))
        .is_err()
    );
}
#[test]
fn text_bytes_and_typed_validation() {
    let id = Svid128::from_u128(0x0123456789abcdef0123456789abcdef);
    assert_eq!(id.to_string(), "01234567-89ab-cdef-0123-456789abcdef");
    assert_eq!(id.to_string().parse::<Svid128>().unwrap(), id);
    assert_eq!(id.to_hex().parse::<Svid128>().unwrap(), id);
    assert_eq!(Svid128::from_be_bytes(id.to_be_bytes()), id);
    for bad in [
        "",
        "1",
        "01234567x89ab-cdef-0123-456789abcdef",
        "01234567-89ab-cdef-0123-456789abcdeg",
    ] {
        assert!(bad.parse::<Svid128>().is_err());
    }
    let reg = Entities::new(false);
    let user: UserId = reg.generate_id();
    assert_eq!(user.0.tag(), Tags::UserId as u16);
    assert!(!user.0.is_client());
    assert_eq!(user.to_string().parse::<UserId>().unwrap(), user);
    assert!(user.to_string().parse::<FileId>().is_err());
    let file: FileId = Entities::new(true).generate_id();
    assert!(file.0.is_client());
    assert_eq!(file.0.tag(), Tags::FileId as u16);
}
#[cfg(feature = "serde")]
#[test]
fn serde_is_lossless_string_and_validates_type() {
    let user: UserId = svid::mint::<UserIdMarker>();
    let json = serde_json::to_string(&user).unwrap();
    assert_eq!(json.len(), 38);
    assert_eq!(serde_json::from_str::<UserId>(&json).unwrap(), user);
    assert!(serde_json::from_str::<FileId>(&json).is_err());
    assert!(serde_json::from_str::<UserId>("256").is_err());
    let max = Svid128::from_u128(u128::MAX);
    assert_eq!(
        serde_json::from_str::<Svid128>(&serde_json::to_string(&max).unwrap()).unwrap(),
        max
    );
}
#[cfg(feature = "autosurgeon")]
#[test]
fn crdt_roundtrip() {
    let user: UserId = svid::mint::<UserIdMarker>();
    let mut doc = automerge::AutoCommit::new();
    autosurgeon::reconcile_prop(&mut doc, automerge::ROOT, "id", &user).unwrap();
    let got: UserId = autosurgeon::hydrate_prop(&doc, automerge::ROOT, "id").unwrap();
    assert_eq!(got, user);
    assert!(autosurgeon::hydrate_prop::<_, FileId, _, _>(&doc, automerge::ROOT, "id").is_err());
}
#[cfg(feature = "http")]
#[test]
fn http_roundtrip() {
    let user: UserId = svid::mint::<UserIdMarker>();
    let header = http::HeaderValue::from(user);
    assert_eq!(UserId::try_from(&header).unwrap(), user);
}
