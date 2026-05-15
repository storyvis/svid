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

## Install

```toml
[dependencies]
svid = "0.4"
# optional: features = ["serde", "diesel", "ts"]
# pick a different bit-layout profile (default is "bits-balanced"):
# svid = { version = "0.4", default-features = false, features = ["bits-high-rand"] }
```

Override the default:

```toml
[dependencies]
svid = { version = "0.4", default-features = false, features = ["bits-high-rand"] }
```

The field order is fixed across profiles: only the timestamp and random bit-widths trade against each other. Other field positions (sign, source, tag) are stable — see the diagram above.


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


## Bit Layout

Default profile (`bits-balanced`):

```text
 63 62                   34 33                            8 7  6     0
┌──┬──────────────────────┬───────────────────────────────┬──┬────────┐
│S0│    TIMESTAMP (T)     │         RANDOM (M)            │W │  TAG  │
│1b│      29 bits         │         26 bits               │1b│ 7 bits│
└──┴──────────────────────┴───────────────────────────────┴──┴────────┘
        T + M = 55 (the timestamp/random tradeoff is set by a Cargo feature)
```

- **Sign (63):** always `0` (keeps `i64` positive).
- **Timestamp:** seconds since `2026-01-01 UTC` (`SVID_EPOCH`). Sits at the top so `ORDER BY id` matches chronological order — the property that makes the format B-tree friendly, same as ULID / Snowflake / UUIDv7.
- **Random:** CSPRNG output (`rand::thread_rng()` = ChaCha12).
- **Source (7):** `0` = server, `1` = client/WASM.
- **Tag (0–6):** 7-bit entity tag (0–127). Anchored at the LSB across **every** profile, so raw bit-ops like `id & 0x7F` stay stable when the bit budget is reallocated. Downstream SQL / JS code that extracts the tag never has to change when you switch profiles. Tag value **127** (`svid::RANDOM_ID_TAG`) is reserved for untyped/random IDs minted via `SvidGenerator::generate_random()`; user-defined `#[derive(Svid)]` enums get a compile-time error if they use it.

### Bit-layout profiles (compile-time)

The timestamp/random trade-off is selected at compile time via Cargo features. Exactly one must be enabled.

| Feature                      | Timestamp | Range from 2026         | Random | 50% collision at | When to use                                  |
|------------------------------|-----------|-------------------------|--------|------------------|----------------------------------------------|
| `bits-long-life`             | 31 bits   | ~68 years (until 2094)  | 24     | ~4.8K IDs/sec    | Archival / long-lived data, low ID rate      |
| **`bits-balanced` (default)**| **29 bits** | **~17 years (until 2043)** | **26** | **~9.6K IDs/sec** | **Recommended for most apps**             |
| `bits-high-rand`             | 28 bits   | ~8.5 years (until 2034) | 27     | ~13.6K IDs/sec   | Short-lived data, high generation rate       |


### Picking a profile

The collision rate is purely a function of the random bits — RNG quality isn't the bottleneck, the bit budget is. Approximate 50%-collision threshold is `√(2 · 2^M · ln 2)`:

| M (random bits) | 50% collision threshold | At 100K IDs/sec, expected collisions |
|-----------------|-------------------------|--------------------------------------|
| 24              | ~4.8K                   | ~287                                 |
| 26              | ~9.6K                   | ~75                                  |
| 27              | ~13.6K                  | ~37                                  |

If you need fully collision-free generation above ~10K IDs/sec, 64-bit isn't enough — you'd need a 128-bit format (ULID / UUIDv7). For most apps, `bits-balanced` is the right point on the curve.




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

## Untyped Random IDs (nanoid / uuidv4 replacement)

When you just need a random ID and don't care about the entity tag, skip the derive entirely:

```rust
use svid::{SvidGenerator, id_to_human_readable};

let id: i64 = SvidGenerator::generate_random(/* is_client = */ false);
let s: String = id_to_human_readable(id);   // fixed 11-char base58
```

These IDs carry `svid::RANDOM_ID_TAG` (`127`) in the tag field, so you can still tell them apart from typed SVIDs after the fact. The remaining bits are exactly the same as any other SVID — chronologically sortable, profile-controlled random width, and source bit.

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
svid   = { version = "0.4", features = ["diesel"] }
diesel = { version = "2", features = ["postgres"] }

[features]
diesel = ["svid/diesel"]
```

## JavaScript / TypeScript

The crate ships JS/TS bindings built with `wasm-bindgen`. IDs cross the FFI as native `bigint` (no precision loss).

A runnable Node.js + TypeScript example lives in [examples/node/](examples/node/).

### Install

```bash
npm install svid
```

The published package ships three `wasm-pack` builds (bundler, nodejs, web) routed via conditional `exports`. Zero-config in Node ≥ 20 and any modern bundler (Vite, Webpack 5, Rollup, esbuild, Parcel). For raw `<script type="module">` use the `svid/web` subpath — see below.

### Build from source

```bash
npm run build           # builds all three targets: pkg/, pkg-node/, pkg-web/
```

Individual targets: `npm run build:bundler`, `npm run build:node`, `npm run build:web`.

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
extractRandomBits(id);           // number, profile-dependent random width
svidEpoch();                     // 1767225600n
decodeBase58(b) === id;          // true
```


### Browser without a bundler

```html
<script type="module">
  import init, { generateSvid } from "https://esm.sh/svid/web";
  await init();
  console.log(generateSvid(1));
</script>
```

The `svid/web` subpath ships the `--target web` build, which requires an explicit `await init()` before any other call. The default `import "svid"` path is for Node and bundlers and auto-initializes.

### Notes

- Time uses `js_sys::Date::now()`; randomness uses `getrandom`'s `js` backend.
- `generateSvid` always sets `isClient = true` (the WASM build runs in the client). To mint server-source IDs from JS, use the low-level `encodeSvid` packer.
- `IdRegistry` and the strongly-typed newtype derives are Rust-only — JS works with raw `bigint`s plus the `extract*` helpers for tag-based dispatch.
- Node ≥ 20 and modern bundlers are zero-config. For raw `<script type="module">`, use the `svid/web` subpath and `await init()`.

## Limitations

- **Epoch:** depends on the selected profile — `bits-long-life` wraps in **2094**, `bits-balanced` (default) in **2043**, `bits-high-rand` in **2034**.
- **Tags:** 7 bits ⇒ max **128** entity types, of which **127** is reserved for `RANDOM_ID_TAG` — leaving **127** usable values for `#[derive(Svid)]` enums (`0..=126`).
- **Collisions:** random width is profile-dependent (24–27 bits). At the default 26 bits, 50% birthday-bound is ~**9,600 IDs/sec/tag**; plan retries above that. For higher rates, switch to `bits-high-rand` or move to a 128-bit format.
- **Source bit:** 1 bit only (server vs client).
- **No reserved bits** — format changes are breaking. Switching profiles is also a wire-format change; pick once per deployment.


## Citation

If you use `svid` in academic or technical work, please cite it:

```bibtex
@software{svid_2026,
  author  = {Bokam, Lava},
  title   = {{svid}: Stateless Verifiable IDs},
  year    = {2026},
  url     = {https://github.com/storyvis/svid},
  license = {Apache-2.0},
  version = {0.4.0}
}
```

GitHub renders a *Cite this repository* button from [CITATION.cff](CITATION.cff).

