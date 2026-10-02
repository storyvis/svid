//! Micro-benchmark: `cargo run --release --example bench`.
//! Reports ns/op (per-thread latency for the multi-threaded rows).

use std::hint::black_box;
use std::time::Instant;

use svid::{SvidGenerator, human_readable_to_id, id_to_human_readable};

const N: u64 = 2_000_000;

fn bench(name: &str, mut f: impl FnMut(u64)) {
    for i in 0..N / 10 {
        f(i);
    }
    let t = Instant::now();
    for i in 0..N {
        f(i);
    }
    println!(
        "{name:<40} {:>8.2} ns/op",
        t.elapsed().as_nanos() as f64 / N as f64
    );
}

fn bench_mt(name: &str, threads: usize, f: impl Fn(usize) -> i64 + Sync) {
    let per = N / 4;
    let t = Instant::now();
    std::thread::scope(|s| {
        for th in 0..threads {
            let f = &f;
            s.spawn(move || {
                for _ in 0..per {
                    black_box(f(th));
                }
            });
        }
    });
    println!(
        "{name:<40} {:>8.2} ns/op",
        t.elapsed().as_nanos() as f64 / per as f64
    );
}

fn main() {
    bench("generate (default)", |_| {
        black_box(SvidGenerator::generate(black_box(1), false));
    });
    bench_mt("generate (default) x8 threads", 8, |_| {
        SvidGenerator::generate(1, false)
    });
    // BEGIN-NEW
    bench("generate_monotonic", |_| {
        black_box(SvidGenerator::generate_monotonic(black_box(1), false));
    });
    bench_mt("generate_monotonic x8 same tag", 8, |_| {
        SvidGenerator::generate_monotonic(1, false)
    });
    bench_mt("generate_monotonic x8 distinct tags", 8, |t| {
        SvidGenerator::generate_monotonic(t as u16, false)
    });
    bench("Svid128::generate", |_| {
        black_box(svid::Svid128::generate(black_box(1)));
    });
    // END-NEW

    let ids: Vec<i64> = (0..1024)
        .map(|_| SvidGenerator::generate(1, false))
        .collect();
    let strs: Vec<String> = ids.iter().map(|&id| id_to_human_readable(id)).collect();

    bench("to_str (id_to_human_readable)", |i| {
        black_box(id_to_human_readable(black_box(ids[(i & 1023) as usize])));
    });
    // BEGIN-NEW
    bench("encode_str_into", |i| {
        let mut buf = [0u8; 11];
        black_box(svid::encode_str_into(
            black_box(ids[(i & 1023) as usize]),
            &mut buf,
        ));
    });
    bench("encode_hex16_into", |i| {
        let mut buf = [0u8; 16];
        black_box(svid::encode_hex16_into(
            black_box(ids[(i & 1023) as usize]),
            &mut buf,
        ));
    });
    let t = svid::Svid128::generate(1);
    bench("Svid128::encode_hex_into", |_| {
        let mut buf = [0u8; 32];
        black_box(black_box(t).encode_hex_into(&mut buf));
    });
    // END-NEW
    bench("parse (human_readable_to_id)", |i| {
        black_box(human_readable_to_id(black_box(&strs[(i & 1023) as usize])).unwrap());
    });
}
