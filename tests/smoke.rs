use std::str::FromStr;

#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SvidTag {
    UserId = 1,
    GroupId = 2,
    FolderId = 3,
    SharedFolderId = 4,
}

svid::define_id!(UserId);
svid::define_id!(GroupId);
svid::define_id!(FolderId);
svid::define_id!(SharedFolderId);

svid::define_id_registry!(IdRegistry { UserId, GroupId, FolderId, SharedFolderId });

svid::define_domain_enum! {
    /// Folder-shaped IDs.
    FolderEnum, "folder" {
        Folder(FolderId),
        Shared(SharedFolderId),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyId {
    UserId(UserId),
    GroupId(GroupId),
    FolderId(FolderId),
    SharedFolderId(SharedFolderId),
}

impl From<UserId> for AnyId {
    fn from(id: UserId) -> Self { AnyId::UserId(id) }
}
impl From<GroupId> for AnyId {
    fn from(id: GroupId) -> Self { AnyId::GroupId(id) }
}
impl From<FolderId> for AnyId {
    fn from(id: FolderId) -> Self { AnyId::FolderId(id) }
}
impl From<SharedFolderId> for AnyId {
    fn from(id: SharedFolderId) -> Self { AnyId::SharedFolderId(id) }
}

svid::define_enum_bridge!(FolderEnum -> AnyId {
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
fn extract_tag_from_i64() {
    use svid::SvidExt;
    let id = svid::SvidGenerator::generate(SvidTag::UserId as u8, false);
    assert_eq!(id.tag(), SvidTag::UserId as u8);
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
        assert!(msg.contains("FolderEnum") || msg.contains("folder"), "{}", msg);
    }
}
