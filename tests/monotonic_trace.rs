use std::collections::HashSet;

use rand::Rng;
use svid::{
    HUMAN_READABLE_LEN, Sequencer, SvidExt, SvidGenerator, TraceId128 as Svid128,
    encode_hex16_into, encode_str_into, hex16_to_id, human_readable_to_id, id_to_hex16,
    id_to_human_readable,
};

#[derive(svid::Svid, Copy, Clone, PartialEq, Eq, Debug)]
#[svid(registry = Reg)]
#[repr(u8)]
pub enum Tag {
    ReqId = 1,
    TurnId = 2,
}

#[derive(svid::SvidDomain, Debug, Clone, Copy, PartialEq, Eq)]
#[svid(error_label = "any", tag = Tag)]
pub enum AnyId {
    Req(ReqId),
    Turn(TurnId),
}

fn is_lower_hex(s: &str, n: usize) -> bool {
    s.len() == n
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[test]
fn monotonic_unique_and_ordered_across_threads_same_second() {
    const THREADS: usize = 8;
    const PER: usize = 50_000;
    let seq = Sequencer::new();
    let now = SvidGenerator::current_timestamp();
    let per_thread: Vec<Vec<i64>> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..THREADS)
            .map(|_| {
                s.spawn(|| {
                    (0..PER)
                        .map(|_| seq.generate_at(1, false, now))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut all = HashSet::new();
    for ids in &per_thread {
        assert!(
            ids.windows(2).all(|w| w[0] < w[1]),
            "per-thread strictly increasing"
        );
        for &id in ids {
            assert!(id > 0);
            assert_eq!(id.tag(), 1);
            assert!(
                all.insert((id.timestamp_bits(), id.random_bits())),
                "duplicate {id}"
            );
        }
    }
    assert_eq!(all.len(), THREADS * PER);
}

#[test]
fn global_monotonic_unique_across_threads() {
    let ids: Vec<i64> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..8)
            .map(|_| {
                s.spawn(|| {
                    (0..20_000)
                        .map(|_| SvidGenerator::generate_monotonic(3, false))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    let set: HashSet<_> = ids.iter().copied().collect();
    assert_eq!(set.len(), ids.len());
}

#[test]
fn clock_clamp_with_injected_clock() {
    let seq = Sequencer::new();
    let a = seq.generate_at(1, false, 1000);
    let b = seq.generate_at(1, false, 990); // clock stepped back 10 s
    assert_eq!(b.timestamp_bits(), 1000);
    assert!(b > a);
    // Other tags are clamped too.
    let c = seq.generate_at(2, false, 995);
    assert_eq!(c.timestamp_bits(), 1000);
    // Clock moves forward again.
    let d = seq.generate_at(1, false, 1001);
    assert_eq!(d.timestamp_bits(), 1001);
    assert!(d > b);
}

#[test]
fn registry_new_monotonic() {
    let reg = Reg::new_monotonic(false);
    let a: ReqId = reg.generate_id();
    let b: ReqId = reg.generate_id();
    assert!(b.to_i64() > a.to_i64());
    assert_eq!(a.to_i64().tag(), Tag::ReqId as u8);
}

#[test]
fn encode_into_byte_identical_to_bs58_path() {
    let mut rng = rand::rng();
    let mut buf = [0u8; HUMAN_READABLE_LEN];
    let edge = [0i64, 1, 57, 58, 255, 256, 1 << 40, i64::MAX, -1, i64::MIN];
    let samples = edge
        .into_iter()
        .chain((0..100_000).map(|_| rng.random::<i64>()));
    for id in samples {
        let reference = {
            let s = bs58::encode(id.to_be_bytes()).into_string();
            format!("{}{}", "1".repeat(HUMAN_READABLE_LEN - s.len()), s)
        };
        assert_eq!(encode_str_into(id, &mut buf), reference, "id {id}");
        assert_eq!(id_to_human_readable(id), reference);
        if id >= 0 {
            assert_eq!(human_readable_to_id(&reference), Ok(id));
            let var = bs58::encode(id.to_be_bytes()).into_string();
            assert_eq!(svid::decode_i64_base58(&var), Ok(id));
        } else {
            assert!(human_readable_to_id(&reference).is_err());
        }
    }
}

#[test]
fn decode_errors_unchanged() {
    // Invalid char / overlong inputs fall back to the bs58 path's messages.
    assert!(
        svid::decode_i64_base58("abc0")
            .unwrap_err()
            .contains("invalid character")
    );
    let over = svid::decode_i64_base58("zzzzzzzzzzzzzz").unwrap_err();
    assert!(over.contains("expected <= 8"), "{over}");
    assert_eq!(svid::decode_i64_base58(""), Ok(0));
    assert_eq!(svid::decode_i64_base58("1111111111111111"), Ok(0));
}

#[test]
fn display_matches_to_str() {
    let reg = Reg::new(false);
    let r: ReqId = reg.generate_id();
    assert_eq!(r.to_string(), r.to_str());
    let mut buf = [0u8; HUMAN_READABLE_LEN];
    assert_eq!(r.encode_into(&mut buf), r.to_str());
    let any = AnyId::from(r);
    assert_eq!(any.to_string(), r.to_str());
    assert_eq!(any.encode_into(&mut buf), r.to_str());
}

#[test]
fn hex16_format_and_roundtrip() {
    for _ in 0..10_000 {
        let id = SvidGenerator::generate(1, false);
        let h = id_to_hex16(id);
        assert!(is_lower_hex(&h, 16), "{h}");
        assert_ne!(h, "0000000000000000");
        assert_eq!(h, format!("{:016x}", id));
        assert_eq!(hex16_to_id(&h), Ok(id));
        assert_eq!(encode_hex16_into(id, &mut [0u8; 16]), h);
    }
    let r: ReqId = Reg::new(false).generate_id();
    assert_eq!(r.to_hex16(), format!("{:016x}", r.to_i64()));
    assert!(hex16_to_id("8000000000000000").is_err());
    assert!(hex16_to_id("ABCDEF0123456789").is_err());
    assert!(hex16_to_id("abc").is_err());
}

#[test]
fn svid128_hex_roundtrip_nonzero_w3c() {
    let mut prev = None;
    for _ in 0..10_000 {
        let t = Svid128::generate(5);
        let h = t.to_hex32();
        assert!(is_lower_hex(&h, 32), "{h}");
        assert_ne!(h, "0".repeat(32));
        assert_eq!(h, t.to_hex());
        assert_eq!(h, t.to_string());
        assert_eq!(h, format!("{:032x}", t.as_u128()));
        assert_eq!(Svid128::from_hex(&h), Ok(t));
        assert_eq!(h.parse::<Svid128>(), Ok(t));
        assert_eq!(t.tag(), 5);
        assert!(t.millis() > 0);
        if let Some(p) = prev {
            assert_ne!(p, t);
        }
        prev = Some(t);
    }
    assert!(Svid128::from_hex(&"0".repeat(32)).is_err());
    assert!(Svid128::from_hex(&"A".repeat(32)).is_err());
    assert!(Svid128::from_u128(0).is_none());
    let p = Svid128::from_parts(123, 7, Svid128::RANDOM_MASK).unwrap();
    assert_eq!(
        (p.millis(), p.tag(), p.random_bits()),
        (123, 7, Svid128::RANDOM_MASK)
    );
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    assert!((Svid128::generate_random().unix_millis() - now_ms).abs() < 5_000);
}

#[test]
fn traceparent_format() {
    let t = Svid128::generate(1);
    let span = svid::SpanId64::generate();
    let mut buf = [0u8; 55];
    let tp = t.encode_traceparent_into(span, 0x01, &mut buf);
    assert_eq!(
        tp,
        format!("00-{}-{}-01", t.to_hex(), id_to_hex16(span.as_u64() as i64))
    );
}

#[test]
fn trace_context_parse() {
    use svid::{SpanId64, TraceContext};
    let w3c = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
    let tc = TraceContext::parse(w3c).unwrap();
    assert_eq!(tc.trace_id.as_u128(), 0x4bf92f3577b34da6a3ce929d0e0e4736);
    assert_eq!(tc.parent_id.as_u64(), 0x00f067aa0ba902b7);
    assert!(tc.sampled());
    assert_eq!(tc.to_string(), w3c);
    assert_eq!(w3c.parse::<TraceContext>(), Ok(tc));
    // Future versions may append fields.
    let v1 = "01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00-extra";
    assert!(!TraceContext::parse(v1).unwrap().sampled());
    for bad in [
        "",
        &w3c[..54],
        &format!("{w3c}-x"),
        "ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        "00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01",
        "00-00000000000000000000000000000000-00f067aa0ba902b7-01",
        "00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01",
        "00_4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        "01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01x",
        "é0-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-0",
    ] {
        assert!(TraceContext::parse(bad).is_err(), "{bad}");
    }
    let s = SpanId64::generate();
    assert_eq!(s.to_string().parse::<SpanId64>(), Ok(s));
    assert_eq!(s.to_hex().len(), SpanId64::HEX_LEN);
    assert!(Svid128::try_generate(4096).is_none());
    assert_eq!(Svid128::try_generate(4095).unwrap().tag(), 4095);
    assert_eq!(Svid128::generate(4096 + 3).tag(), 3);
}

#[cfg(feature = "http")]
#[test]
fn header_value_conversions() {
    use svid::http::HeaderValue;
    let r: ReqId = Reg::new(false).generate_id();
    let hv = HeaderValue::from(r);
    assert_eq!(hv.to_str().unwrap(), r.to_str());
    assert_eq!(ReqId::try_from(&hv), Ok(r));
    let any = AnyId::from(r);
    assert_eq!(AnyId::try_from(&HeaderValue::from(any)), Ok(any));
    assert!(TurnId::try_from(&hv).is_err());

    let t = Svid128::generate(1);
    let hv = HeaderValue::from(t);
    assert_eq!(hv.to_str().unwrap(), t.to_hex());
    assert_eq!(Svid128::try_from(&hv), Ok(t));
}

#[cfg(feature = "monotonic")]
#[test]
fn monotonic_feature_routes_generate() {
    let a = SvidGenerator::generate(9, false);
    let b = SvidGenerator::generate(9, false);
    assert!(b > a);
}
