# svid

Typed entity IDs for Rust and WASM, with separate W3C trace and span IDs.
SVID64 keeps the 0.5.x wire format: existing IDs, strings and tags stay valid.

## Entity layouts

| Format | High → low bits | Storage | Text |
|---|---|---|---|
| SVID64 (default profile) | sign 1 / seconds 29 / random 26 / source 1 / type 7 | positive BIGINT, 8 bytes | 11-character base58 |
| SVID128 (UUIDv8) | Unix ms 48 / version 4 / random 12 / variant 2 / random 49 / source 1 / type 12 | PostgreSQL UUID, 16 bytes | 36-character UUID text |

SVID64: tags 0–126, 127 is reserved for untyped random IDs; extract with
`id & 0x7F`, source is bit 7. Enums use `#[repr(u8)]` (or `u16`).

SVID128: tags 0–4094, 4095 is reserved; extract the low 12 bits of the final two
bytes in network byte order, source is bit 12. Enums use `#[repr(u16)]`.

SVID128 is an RFC 9562 UUIDv8 (version `8`, variant `10`), so UUID validators
accept it. Parsing rejects other versions and the nil UUID, and accepts either
case. It has 48-bit Unix milliseconds (about 8,919 years from 1970). Ordering
groups IDs by millisecond; within a millisecond their order is random. Clock
rollback can move new IDs behind older ones. IDs do not replace creation
timestamps or sequence cursors.

The 61 random bits provide distributed collision resistance for each
millisecond/type/source bucket. At one million IDs in a single bucket, the
birthday approximation is 2.2×10^-7 collision probability. Database uniqueness
constraints are still required. Source denotes client/server, not a worker ID.

## Typed IDs

```rust
#[derive(svid::Svid, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
#[svid(registry = EntityIds, bits = 128)]
pub enum Tag { UserId = 1, AssetId = 300 }

let user: UserId = svid::mint::<UserIdMarker>();
let text = user.to_string();
assert_eq!(text.parse::<UserId>().unwrap(), user);
assert!(text.parse::<AssetId>().is_err());
```

Omit `bits = 128` for SVID64. Use the same width attribute on `SvidDomain`.
The derive rejects, at compile time, tags that do not fit the width or use the
reserved tag. Typed string parsing checks the tag. `to_raw()` exposes the
underlying `i64` or `Svid128`. `to_str()` and `from_str_id()` use the format's
canonical text. Serde and HTTP adapters use strings, avoiding JavaScript number
precision loss. Autosurgeon stores SVID128 as strings and SVID64 as integers.
Diesel and postgres-types map SVID128 to UUID. `encode_into` writes into
caller-provided buffers without allocating; owned strings and HTTP header
values may allocate.

## SVID64 profiles and monotonic generation

Exactly one layout profile must be enabled:

| Feature | Timestamp seconds since 2026 | Last encodable second | Random bits |
|---|---:|---|---:|
| `bits-long-life` | 31 | 2094-01-19T03:14:07Z | 24 |
| `bits-balanced` (default) | 29 | 2043-01-05T18:48:31Z | 26 |
| `bits-high-rand` | 28 | 2034-07-04T21:24:15Z | 27 |

Use `default-features = false` when selecting a different profile. All
producers/consumers of SVID64 must agree on the profile. Prefer SVID128 for
distributed durable identities.

`Sequencer`, `generate_monotonic`, and the `monotonic` feature provide
process-local SVID64 ordering and uniqueness per tag. They do not coordinate
separate processes or survive restart. When the random field is exhausted
within a second, the sequencer advances logical time. Past the profile's last
second, `generate` wraps like 0.5.x and `try_generate` returns `None`. The
128-tag sequencer is about 8 KiB. SVID128 generation keeps independent random
bits; `IdGenerator::new_monotonic` exists only for SVID64 kinds.

## Tracing

`TraceId128` uses `[milliseconds since 2026:41][type:12][random:75]`.
Its metadata positions intentionally differ from entity IDs so its rightmost
56 bits stay random for W3C Trace Context. It formats as 32 lowercase hex
characters. `SpanId64` uses all 64 random bits and excludes zero.
`TraceContext::parse` reads a `traceparent` header (version rules, lowercase
hex, non-zero IDs) and `TraceContext::encode_into` writes one without
allocating. Entity IDs must not be used directly as W3C trace IDs.

## Features and WASM

Optional features: `serde`, `autosurgeon`, `diesel`, `postgres`, `strum`, `ts`,
`http`, `monotonic`. WASM provides `generateSvid128`, `normalizeSvid128`, and
`extractSvid128Tag` using strings, alongside the SVID64 BigInt functions.
Out-of-range tags or fields throw a JS `Error` instead of aborting.
