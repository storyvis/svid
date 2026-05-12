# svid (Stateless Verifiable IDs)

> 64-bit domain-typed, stateless, verifiable IDs for native and WASM.

`svid` generates `i64` IDs that are chronologically sortable, carry a 7-bit entity tag, and need zero coordination between processes.

## Bit Layout

```text
 63 62              32 31 30      24 23                             0
┌──┬──────────────────┬──┬──────────┬────────────────────────────────┐
│S0│    TIMESTAMP     │W │ ID TYPE  │            RANDOM              │
│1b│     31 bits      │1b│  7 bits  │            24 bits             │
└──┴──────────────────┴──┴──────────┴────────────────────────────────┘
```

- **Sign (63):** always `0`.
- **Timestamp (32–62):** seconds since `2026-01-01 UTC` (`SVID_EPOCH`).
- **Source (31):** `0` = server, `1` = client/WASM.
- **ID Type (24–30):** 7-bit entity tag (0–127).
- **Random (0–23):** 24 bits of CSPRNG output.

## Install

```toml
[dependencies]
svid = "0.1"
# optional: features = ["serde", "diesel", "ts"]
```

## Quick Start

```rust
use std::str::FromStr;

#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SvidTag { UserId = 1, GroupId = 2 }

svid::define_id!(UserId);
svid::define_id!(GroupId);
svid::define_id_registry!(IdRegistry { UserId, GroupId });

let reg = IdRegistry::new(/* is_client = */ false);
let u: UserId = reg.user_id.generate_id();
let g: GroupId = reg.group_id.generate_id();

// Tag is checked on parse.
assert_eq!(UserId::from_str(&u.to_string()).unwrap(), u);
assert!(UserId::from_str(&g.to_string()).is_err());
```

The `SvidTag` enum must be in scope at every `define_id!` site, must be `#[repr(u8)]`, and variant names must match newtype names exactly. Tag values are embedded in persisted IDs — never reuse them.

## Encoding

```rust
let u: UserId = reg.user_id.generate_id();

let b: String = u.to_base58();              // variable-length base58
let h: String = u.to_str();                 // fixed 11-char base58
let n: i64    = u.to_i64();                 // raw — no tag check

UserId::from_base58(&b)?;                   // tag-checked
UserId::from_str_id(&h)?;                   // tag-checked
let _: UserId = UserId::from(n);            // unchecked

// Display / FromStr auto-dispatch by length (11 → str_id, else base58).
let _: UserId = u.to_string().parse()?;
```

## Domain Enums

Group related IDs into one type:

```rust
svid::define_domain_enum! {
    FolderEnum, "folder" {
        Folder(FolderId),
        Shared(SharedFolderId),
    }
}

let f: FolderId = reg.folder_id.generate_id();
let any: FolderEnum = f.into();
let _: FolderEnum = FolderEnum::from_i64(f.to_i64())?;
let _: FolderId = f.try_into()?;            // recover inner newtype
```

`from_i64` rejects IDs whose tag isn't in the domain.

## Bridging to a Wider Enum

```rust
pub enum AnyId { UserId(UserId), FolderId(FolderId), SharedFolderId(SharedFolderId) }

impl From<UserId>         for AnyId { fn from(x: UserId)         -> Self { Self::UserId(x) } }
impl From<FolderId>       for AnyId { fn from(x: FolderId)       -> Self { Self::FolderId(x) } }
impl From<SharedFolderId> for AnyId { fn from(x: SharedFolderId) -> Self { Self::SharedFolderId(x) } }

svid::define_enum_bridge!(FolderEnum -> AnyId {
    Folder(FolderId),
    Shared(SharedFolderId),
});

let any: AnyId = FolderEnum::Folder(f).into();
```

## Inspecting Raw IDs

```rust
use svid::SvidExt;

let id: i64 = u.to_i64();
id.tag();              // u8
id.unix_timestamp();   // i64
id.is_client();        // bool
id.random_bits();      // u32

let dec = svid::DecomposedSvid::from_i64(id);
```

## Features

| Feature  | Adds |
|----------|------|
| `serde`  | string-based `Serialize` / `Deserialize` |
| `diesel` | `ToSql` / `FromSql` for `BigInt` on Postgres |
| `ts`     | `#[derive(TS)]` for ts-rs TypeScript export |

The macros emit `#[cfg(feature = "…")]` impls that resolve against **your crate's** features — mirror them in your `Cargo.toml`:

```toml
[dependencies]
svid   = { version = "0.1", features = ["diesel"] }
diesel = { version = "2", features = ["postgres"] }

[features]
diesel = ["svid/diesel"]
```

## JavaScript / TypeScript

The crate ships JS/TS bindings built with `wasm-bindgen`. IDs cross the FFI as native `bigint` (no precision loss).

### Build

```bash
npm run build           # wraps: wasm-pack build --target bundler --release

```


### Usage

```ts
import {
    generateSvid,
    decodeSvid,
    encodeHumanReadable,
    decodeHumanReadableExpecting,
    encodeBase58,
    decodeBase58,
    extractTag,
    svidEpoch,
} from "svid";

const USER_ID_TAG = 1;

const id: bigint = generateSvid(USER_ID_TAG);

const s: string = encodeHumanReadable(id);           // fixed 11-char base58
const b: string = encodeBase58(id);                  // variable-length base58

const back: bigint = decodeHumanReadableExpecting(s, USER_ID_TAG); // throws on tag mismatch
console.assert(back === id);

const d = decodeSvid(id);
// { timestamp: number, isClient: true, idType: 1, random: number, unixTimestamp: bigint }

extractTag(id);          // 1
svidEpoch();             // 1767225600n
decodeBase58(b) === id;  // true
```


### Notes

- Time uses `js_sys::Date::now()`; randomness uses `getrandom`'s `js` backend.
- `generateSvid` always sets `isClient = true` (the WASM build runs in the client). To mint server-source IDs from JS, use the low-level `encodeSvid` packer.
- `IdRegistry` and the strongly-typed newtype macros are Rust-only — JS works with raw `bigint`s plus the `extract*` helpers for tag-based dispatch.

## Limitations

- **Epoch:** 31-bit seconds + 2026 epoch ⇒ wraps in **January 2094**.
- **Tags:** 7 bits ⇒ max **128** entity types.
- **Collisions:** 24-bit random ⇒ 50% birthday-bound at ~**5,100 IDs/sec/tag**. Plan retries above that.
- **Source bit:** 1 bit only (server vs client).
- **No reserved bits** — format changes are breaking.
