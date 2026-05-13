use std::str::FromStr;

#[derive(svid::Svid, Copy, Clone, PartialEq, Eq, Debug)]
#[svid(registry = IdRegistry)]
#[repr(u8)]
pub enum SvidTag {
    UserId = 1,
    GroupId = 2,
    FolderId = 3,
    SharedFolderId = 4,
}

/// Folder-shaped IDs.
#[derive(svid::SvidDomain, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[svid(error_label = "folder")]
pub enum FolderEnum {
    Folder(FolderId),
    Shared(SharedFolderId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyId {
    UserId(UserId),
    GroupId(GroupId),
    FolderId(FolderId),
    SharedFolderId(SharedFolderId),
}

impl From<UserId> for AnyId {
    fn from(id: UserId) -> Self {
        AnyId::UserId(id)
    }
}
impl From<GroupId> for AnyId {
    fn from(id: GroupId) -> Self {
        AnyId::GroupId(id)
    }
}
impl From<FolderId> for AnyId {
    fn from(id: FolderId) -> Self {
        AnyId::FolderId(id)
    }
}
impl From<SharedFolderId> for AnyId {
    fn from(id: SharedFolderId) -> Self {
        AnyId::SharedFolderId(id)
    }
}

svid::bridge!(FolderEnum -> AnyId {
    Folder(FolderId),
    Shared(SharedFolderId),
});

#[test]
fn newtype_base58_roundtrip() {
    let reg = IdRegistry::new(false);
    let u: UserId = reg.user_id.generate_id();
    let encoded = u.to_base58();
    let decoded = UserId::from_base58(&encoded).expect("base58 roundtrip");
    assert_eq!(u, decoded);
}

#[test]
fn newtype_human_readable_roundtrip() {
    let reg = IdRegistry::new(false);
    let u: UserId = reg.user_id.generate_id();
    let s = u.to_str();
    assert_eq!(s.len(), svid::HUMAN_READABLE_LEN);
    let decoded = UserId::from_str_id(&s).expect("human-readable roundtrip");
    assert_eq!(u, decoded);
}

#[test]
fn newtype_fromstr_dispatch() {
    let reg = IdRegistry::new(false);
    let u: UserId = reg.user_id.generate_id();
    let h = u.to_str();
    let parsed = UserId::from_str(&h).expect("FromStr human-readable");
    assert_eq!(u, parsed);
}

#[test]
fn newtype_rejects_wrong_tag() {
    let reg = IdRegistry::new(false);
    let g: GroupId = reg.group_id.generate_id();
    let s = g.to_base58();
    let err = UserId::from_base58(&s).unwrap_err();
    assert!(err.contains("Invalid SVID tag"), "{}", err);
}

#[test]
fn marker_kind_tag_matches_svid_tag() {
    use svid::SvidKind;
    assert_eq!(<UserIdMarker as SvidKind>::TAG, SvidTag::UserId as u8);
    assert_eq!(<GroupIdMarker as SvidKind>::TAG, SvidTag::GroupId as u8);
}

#[test]
fn domain_enum_roundtrip_and_dispatch() {
    let reg = IdRegistry::new(false);
    let f: FolderId = reg.folder_id.generate_id();
    let e: FolderEnum = f.into();
    assert_eq!(e.tag(), SvidTag::FolderId as u8);

    let s: SharedFolderId = reg.shared_folder_id.generate_id();
    let es: FolderEnum = s.into();
    assert_eq!(es.tag(), SvidTag::SharedFolderId as u8);

    let parsed = FolderEnum::from_i64(f.to_i64()).expect("from_i64");
    assert_eq!(parsed, e);

    let any: AnyId = e.into();
    assert!(matches!(any, AnyId::FolderId(_)));
}

#[test]
fn domain_enum_rejects_unknown_tag() {
    let reg = IdRegistry::new(false);
    let u: UserId = reg.user_id.generate_id();
    let err = FolderEnum::from_i64(u.to_i64()).unwrap_err();
    assert!(err.contains("folder tag"), "{}", err);
}

#[test]
fn registry_infers_id_type_from_binding() {
    use svid::SvidExt;
    let reg = IdRegistry::new(false);
    let u: UserId = reg.generate_id();
    let g: GroupId = reg.generate_id();
    assert_eq!(u.to_i64().tag(), SvidTag::UserId as u8);
    assert_eq!(g.to_i64().tag(), SvidTag::GroupId as u8);
}

#[test]
fn extract_tag_from_i64() {
    use svid::SvidExt;
    let id = svid::SvidGenerator::generate(SvidTag::UserId as u8, false);
    assert_eq!(id.tag(), SvidTag::UserId as u8);
}

#[test]
fn bit_layout_sums_to_64() {
    let total = 1u32
        + svid::TIMESTAMP_BITS as u32
        + svid::RANDOM_BITS as u32
        + svid::SOURCE_BITS as u32
        + svid::IDTYPE_BITS as u32;
    assert_eq!(total, 64, "active profile must sum to 64 bits including sign");
}

#[test]
fn tag_is_bit_stable_at_lsb() {
    // Tag extraction must be `id & 0x7F` regardless of compile-time profile —
    // downstream SQL / JS code depends on this property.
    use svid::SvidExt;
    let id = svid::SvidGenerator::generate(5, false);
    assert_eq!(id.tag(), 5);
    assert_eq!((id & 0x7F) as u8, 5);
}

#[test]
fn chronological_sort_preserved() {
    // i64 ordering must match timestamp ordering — required for DB B-tree
    // efficiency (the ULID / Snowflake / UUIDv7 property).
    let max_rand = svid::RANDOM_MASK as u32;
    let early = svid::encode_svid(100, false, 1, max_rand);
    let late = svid::encode_svid(200, false, 1, 0);
    assert!(early < late, "i64 ordering must match timestamp ordering");
}

#[test]
fn decode_i64_base58_roundtrips_through_helper() {
    let reg = IdRegistry::new(false);
    let u: UserId = reg.user_id.generate_id();
    let encoded = u.to_base58();
    let raw = svid::decode_i64_base58(&encoded).expect("helper decode");
    assert_eq!(raw, u.to_i64());
}

#[test]
fn from_str_id_rejects_wrong_length() {
    let err = UserId::from_str_id("xy").unwrap_err();
    assert!(
        err.contains("expected") && err.contains("chars"),
        "unexpected error: {}",
        err
    );
}

#[test]
fn from_base58_rejects_overlong_input_with_unified_message() {
    let too_long = svid::bs58::encode(&[0xFFu8; 9]).into_string();
    let err = UserId::from_base58(&too_long).unwrap_err();
    assert!(
        err.contains("invalid base58 SVID"),
        "expected unified error message, got: {}",
        err
    );
}

#[cfg(feature = "autosurgeon")]
mod autosurgeon_smoke {
    use super::*;
    use autosurgeon::{hydrate, reconcile, Hydrate, Reconcile};

    #[derive(Reconcile, Hydrate, Debug, PartialEq)]
    struct Doc {
        user: UserId,
        folder: FolderEnum,
    }

    #[test]
    fn newtype_and_domain_enum_roundtrip_through_automerge() {
        let reg = IdRegistry::new(false);
        let user: UserId = reg.user_id.generate_id();
        let folder: FolderId = reg.folder_id.generate_id();
        let doc = Doc {
            user,
            folder: FolderEnum::Folder(folder),
        };

        let mut am = automerge::AutoCommit::new();
        reconcile(&mut am, &doc).expect("reconcile");
        let back: Doc = hydrate(&am).expect("hydrate");
        assert_eq!(back, doc);
    }

    #[test]
    fn domain_enum_hydrate_rejects_wrong_tag() {
        // Round-trip a UserId i64 into the doc, then try to hydrate it as
        // FolderEnum — the TryFrom-based Hydrate should fail with a
        // HydrateError because the tag doesn't match any FolderEnum variant.
        let reg = IdRegistry::new(false);
        let bogus = reg.user_id.generate_id().to_i64();

        #[derive(Reconcile)]
        struct Wrapper {
            value: i64,
        }
        #[derive(Hydrate, Debug)]
        struct WrapperOut {
            #[allow(dead_code)]
            value: FolderEnum,
        }

        let mut am = automerge::AutoCommit::new();
        reconcile(&mut am, &Wrapper { value: bogus }).expect("reconcile");
        let err = hydrate::<_, WrapperOut>(&am).unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("FolderEnum") || msg.contains("folder"),
            "{}",
            msg
        );
    }
}
