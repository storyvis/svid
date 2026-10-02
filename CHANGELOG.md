# Changelog

## 0.6.0

SVID64 wire format is unchanged from 0.5.4 (7-bit tag, same profiles, same
default `bits-balanced`); golden vectors in `tests/compat_054.rs`.

Added:
- `Svid128`: 128-bit entity ID stored as an RFC 9562 UUIDv8 with a 12-bit tag.
- `TraceId128`, `SpanId64`, `TraceContext` (W3C `traceparent` parse/encode).
- `Sequencer` / `SvidGenerator::generate_monotonic` / `monotonic` feature.
- `try_encode_svid`, `Sequencer::try_generate`, `TraceId128::try_generate`.
- Allocation-free `encode_into` / hex helpers; `http` and `postgres` features.
- `TAG128_BITS`, `TAG128_MASK`, `RANDOM_ID_TAG128`.

Changed (breaking):
- `SvidKind::TAG` is `u16` (was `u8`) so one trait covers both widths, and
  `SvidKind` has a new associated type `Raw: SvidValue` (`i64` or `Svid128`);
  `mint` / `IdGenerator` require `Id: From<Raw>`. Hand-written `SvidKind`
  impls must add `type Raw = i64;`.
- `SvidExt::tag()` and the SVID64 APIs keep `u8`; 128-bit APIs use `u16`.
- `#[derive(Svid)]` accepts `#[repr(u8)]` or `#[repr(u16)]` and rejects tags
  that do not fit the width (SVID64: 0..=126; previously 128..=255 were
  silently truncated).
- The monotonic sequencer issues timestamp 1 for a clock at or before
  `SVID_EPOCH` (timestamp 0), so the first ID of each tag is randomly seeded.
- WASM `generateSvid`, `encodeSvid` and `generateSvid128` return `Result`
  (throw a JS `Error`) instead of aborting on out-of-range input; `encodeSvid`
  rejects oversized fields.
