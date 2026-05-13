# svid (Stateless Verifiable IDs)

64-bit domain-typed, stateless, verifiable IDs for native and WASM.

`svid` generates `i64` IDs that are chronologically sortable, carry a 7-bit entity tag, and need zero coordination between processes.

## Why SVID?

- **Compact PK** — 64-bit `i64` fits in 8 bytes per row; half the on-disk footprint of a UUID and the natural width for `BIGINT` columns and JS `bigint`.
- **B-tree friendly** — high bits are a monotonic timestamp, so recent inserts cluster at the right edge of the index instead of scattering across the tree the way UUIDv4 does. Reduces page splits and index bloat.
- **Type info travels with the ID** — the 7-bit entity tag lets any service, log line, queue payload, or DB row dispatch on entity kind without a side-table lookup or external schema.
- **Compile-time type safety** — `#[derive(Svid)]` mints distinct newtypes (`UserId`, `GroupId`, …); the Rust compiler refuses to swap them. Parse-time tag mismatches return typed errors instead of corrupting data silently.
- **Chronologically sortable** — `ORDER BY id` is `ORDER BY creation_time` to one-second resolution, often eliminating the need for a separate `created_at` column.
- **Stateless and coordination-free** — server and WASM clients mint IDs locally; the 1-bit source field disambiguates origin without a central allocator.
- **Format-stable** — positive `i64` round-trips losslessly through PostgreSQL `BIGINT`, JSON strings, JS `bigint`, and the base58 wire forms; same bytes, all the way down.

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

#[derive(svid::Svid, Copy, Clone, PartialEq, Eq, Debug)]
#[svid(registry = IdRegistry)]
#[repr(u8)]
pub enum SvidTag { UserId = 1, GroupId = 2 }

let reg = IdRegistry::new(/* is_client = */ false);

// Type-inferred — variant picked from the binding type.
let u: UserId  = reg.generate_id();
let g: GroupId = reg.generate_id();

// Or address the typed generator directly:
let u2: UserId = reg.user_id.generate_id();

// Tag is checked on parse.
assert_eq!(UserId::from_str(&u.to_string()).unwrap(), u);
assert!(UserId::from_str(&g.to_string()).is_err());
```

`#[derive(svid::Svid)]` emits one newtype (`UserId`, `GroupId`, …) and one marker type per variant alongside the enum. Variants must be unit variants with explicit `= N` discriminants in the 0–127 range — those values get persisted inside every ID and **must not be reused or renumbered** later. The `#[svid(registry = ...)]` helper is optional; omit it to skip generating the registry struct.

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
#[derive(svid::SvidDomain, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[svid(error_label = "folder")]
pub enum FolderEnum {
    Folder(FolderId),
    Shared(SharedFolderId),
}

let f: FolderId = reg.generate_id();
let any: FolderEnum = f.into();
let _: FolderEnum = FolderEnum::from_i64(f.to_i64())?;
let _: FolderId = any.try_into()?;          // recover inner newtype
```

Variants must be single-field tuple variants whose inner type is a bare ident matching a `SvidTag` variant (e.g. `Folder(FolderId)` pairs with `SvidTag::FolderId`). `error_label` is interpolated into the error message returned by `from_i64` when the tag doesn't match any variant. If your tag enum isn't named `SvidTag`, add `#[svid(tag = MyTag)]` to point the derive at it.

## Bridging to a Wider Enum

```rust
pub enum AnyId { UserId(UserId), FolderId(FolderId), SharedFolderId(SharedFolderId) }

impl From<UserId>         for AnyId { fn from(x: UserId)         -> Self { Self::UserId(x) } }
impl From<FolderId>       for AnyId { fn from(x: FolderId)       -> Self { Self::FolderId(x) } }
impl From<SharedFolderId> for AnyId { fn from(x: SharedFolderId) -> Self { Self::SharedFolderId(x) } }

svid::bridge!(FolderEnum -> AnyId {
    Folder(FolderId),
    Shared(SharedFolderId),
});

let f: FolderId = reg.generate_id();
let any: AnyId  = FolderEnum::Folder(f).into();
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

The derives emit `#[cfg(feature = "…")]` impls that resolve against **your crate's** features — mirror them in your `Cargo.toml`:

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

extractTag(id);                  // 1
extractIsClient(id);             // true
extractUnixTimestamp(id);        // bigint, seconds since unix epoch
extractTimestampBits(id);        // number, seconds since SVID_EPOCH
extractRandomBits(id);           // number, 24-bit random
svidEpoch();                     // 1767225600n
decodeBase58(b) === id;          // true
```


### Notes

- Time uses `js_sys::Date::now()`; randomness uses `getrandom`'s `js` backend.
- `generateSvid` always sets `isClient = true` (the WASM build runs in the client). To mint server-source IDs from JS, use the low-level `encodeSvid` packer.
- `IdRegistry` and the strongly-typed newtype derives are Rust-only — JS works with raw `bigint`s plus the `extract*` helpers for tag-based dispatch.

## Limitations

- **Epoch:** 31-bit seconds + 2026 epoch ⇒ wraps in **January 2094**.
- **Tags:** 7 bits ⇒ max **128** entity types.
- **Collisions:** 24-bit random ⇒ 50% birthday-bound at ~**5,100 IDs/sec/tag**. Plan retries above that.
- **Source bit:** 1 bit only (server vs client).
- **No reserved bits** — format changes are breaking.


## Citation

If you use `svid` in academic or technical work, please cite it:

```bibtex
@software{svid_2026,
  author  = {Bokam, Lava},
  title   = {{svid}: Stateless Verifiable IDs},
  year    = {2026},
  url     = {https://github.com/storyvis/svid},
  license = {Apache-2.0},
  version = {0.1.0}
}
```

GitHub renders a *Cite this repository* button from [CITATION.cff](CITATION.cff).

