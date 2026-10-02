# svid

Typed entity IDs for Rust and WASM, with separate W3C trace and span IDs.
This branch changes the wire format; existing 7-bit IDs must not be reinterpreted.

## Entity layouts

| Format | High → low bits | Storage | Text |
|---|---|---|---|
| SVID64 (default profile) | sign 1 / seconds 31 / random 19 / source 1 / type 12 | positive BIGINT, 8 bytes | 11-character base58 |
| SVID128 | Unix milliseconds 48 / random 67 / source 1 / type 12 | PostgreSQL UUID, 16 bytes | 36-character UUID text |

Both formats put the type at bits 0–11 and source at bit 12. Tags 0–4094
are available to domains; 4095 is reserved for untyped random IDs. Enums use
`#[repr(u16)]`. Tag extraction uses `id & 4095` for SVID64. For UUID storage,
extract the low 12 bits of the final two bytes in network byte order.

SVID128 uses UUID storage and formatting but is **not UUIDv4, UUIDv7, or an
RFC-versioned UUID**. Do not run UUID version/variant validators on it. It has
48-bit Unix milliseconds (about 8,919 years from 1970). Ordering groups IDs by
millisecond; within a millisecond their order is random. Clock rollback can
move new IDs behind older ones. IDs do not replace creation timestamps or
sequence cursors.

The 67 random bits provide distributed collision resistance for each
millisecond/type/source bucket. At one million IDs in a single bucket, the
birthday approximation is 3.4×10^-9 collision probability. Database uniqueness
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
Typed string parsing checks the tag. `to_raw()` exposes the underlying `i64`
or `Svid128`; raw constructors are deliberately unchecked. `to_str()` and
`from_str_id()` use the format's canonical text. Serde and HTTP adapters use
strings, avoiding JavaScript number precision loss. Autosurgeon stores SVID128
as strings and SVID64 as integers. The Diesel adapter maps SVID128 to UUID.
`encode_into` writes into caller-provided buffers without allocating; creating
owned strings and HTTP header values may allocate.

## SVID64 profiles and monotonic generation

Exactly one layout profile must be enabled:

| Feature | Timestamp seconds since 2026 | Random bits |
|---|---:|---:|
| `bits-long-life` (default) | 31 (~68 years) | 19 |
| `bits-balanced` | 29 (~17 years) | 21 |
| `bits-high-rand` | 28 (~8.5 years) | 22 |

Use `default-features = false` when selecting a different profile. All
producers/consumers of SVID64 must agree on the profile. Wider type metadata
reduces its random budget: prefer SVID128 for distributed durable identities.

`Sequencer`, `generate_monotonic`, and the `monotonic` feature provide process-local
SVID64 ordering and uniqueness per tag/source. They do not coordinate separate
processes or survive restart. On exhaustion the sequencer advances logical time;
timestamp overflow fails instead of wrapping. The 4096-tag sequencer reserves
about 256 KiB. SVID128 generation retains independent random bits; it has no
monotonic registry constructor.

## Tracing

`TraceId128` uses `[milliseconds since 2026:41][type:12][random:75]`.
Its metadata positions intentionally differ from entity IDs so its rightmost
56 bits stay random for W3C Trace Context. It formats as 32 lowercase hex
characters. `SpanId64` uses all 64 random bits and excludes zero. Traceparent
encoding accepts `SpanId64`, preventing an all-zero span in release builds.
Entity IDs must not be used directly as W3C trace IDs.

## Features and WASM

Optional features: `serde`, `autosurgeon`, `diesel`, `strum`, `ts`, `http`,
`monotonic`. WASM provides `generateSvid128`, `normalizeSvid128`, and
`extractSvid128Tag` using strings, alongside the SVID64 BigInt functions.
