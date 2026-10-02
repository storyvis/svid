use svid::Svid128;
#[derive(svid::Svid, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u16)]
#[svid(registry = Entities, bits = 128)]
pub enum Tags {
    UserId = 256,
    FileId = 4094,
}

#[test]
fn layout_and_boundaries() {
    for tag in [0u16, 127, 128, 255, 256, 2048, 4094, 4095] {
        for client in [false, true] {
            let id = Svid128::from_parts(17, client, tag, 123).unwrap();
            assert_eq!((id.tag(), id.is_client()), (tag, client));
            assert_eq!((id.millis(), id.random_bits()), (17, 123));
            assert_eq!(id.as_u128() & 0xfff, tag as u128);
            assert_eq!(id.as_u128() >> 12 & 1, client as u128);
        }
    }
    let max = Svid128::from_parts(Svid128::MILLIS_MASK, true, 4095, Svid128::RANDOM_MASK).unwrap();
    assert_eq!(max.random_bits(), Svid128::RANDOM_MASK);
    // RFC 9562: version 8, variant 0b10.
    for id in [
        max,
        Svid128::from_parts(0, false, 0, 0).unwrap(),
        Svid128::generate(1),
    ] {
        let text = id.to_string();
        assert_eq!(&text[14..15], "8");
        assert!(matches!(&text[19..20], "8" | "9" | "a" | "b"));
    }
    assert_eq!(max.to_string(), "ffffffff-ffff-8fff-bfff-ffffffffffff");
    assert!(Svid128::from_parts(0, false, 4096, 0).is_none());
    assert!(Svid128::from_parts(Svid128::MILLIS_MASK + 1, false, 0, 0).is_none());
    assert!(Svid128::from_parts(0, false, 0, Svid128::RANDOM_MASK + 1).is_none());
    // Generation masks oversized tags instead of panicking.
    assert_eq!(Svid128::generate(4096 + 5).tag(), 5);
    assert_eq!(Svid128::generate_random().tag(), svid::RANDOM_ID_TAG128);
}
#[test]
fn ordering_follows_time() {
    let a = Svid128::from_parts(1000, true, 4095, Svid128::RANDOM_MASK).unwrap();
    let b = Svid128::from_parts(1001, false, 0, 0).unwrap();
    assert!(a < b);
    assert!(a.to_string() < b.to_string());
}
#[test]
fn text_bytes_and_typed_validation() {
    let id = Svid128::from_u128(0x0123456789ab8def_a123456789abcdef).unwrap();
    assert_eq!(id.to_string(), "01234567-89ab-8def-a123-456789abcdef");
    assert_eq!(id.to_string().parse::<Svid128>().unwrap(), id);
    assert_eq!(id.to_hex().parse::<Svid128>().unwrap(), id);
    assert_eq!(
        "01234567-89AB-8DEF-A123-456789ABCDEF"
            .parse::<Svid128>()
            .unwrap(),
        id
    );
    assert_eq!(Svid128::from_be_bytes(id.to_be_bytes()), Some(id));
    assert!(Svid128::from_u128(0).is_none());
    assert!(Svid128::from_u128(u128::MAX).is_none());
    assert!(Svid128::from_be_bytes([0; 16]).is_none());
    for bad in [
        "",
        "1",
        "00000000-0000-0000-0000-000000000000",
        // UUIDv4 / v7 are not SVID128.
        "01234567-89ab-4def-a123-456789abcdef",
        "01234567-89ab-7def-a123-456789abcdef",
        "01234567-89ab-8def-c123-456789abcdef",
        "01234567x89ab-8def-a123-456789abcdef",
        "01234567-89ab-8def-a123-456789abcdeg",
        "0123456é-89ab-8def-a123-456789abcde",
    ] {
        assert!(bad.parse::<Svid128>().is_err(), "{bad}");
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
    let max = Svid128::from_parts(Svid128::MILLIS_MASK, true, 4095, Svid128::RANDOM_MASK).unwrap();
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
