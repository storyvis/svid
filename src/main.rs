use std::io::{self, BufWriter, Write};
use std::process::ExitCode;

use svid::{id_to_human_readable, DecomposedSvid, SvidGenerator};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let prog = "svid";

    let idtype_arg = match args.next() {
        Some(a) => a,
        None => return usage(prog, "missing <idtype>"),
    };
    let count_arg = match args.next() {
        Some(a) => a,
        None => return usage(prog, "missing <count>"),
    };
    if args.next().is_some() {
        return usage(prog, "too many arguments");
    }

    let idtype: u8 = match idtype_arg.parse::<u32>() {
        Ok(v) if v <= 127 => v as u8,
        _ => return usage(prog, "idtype must be an integer in 0..=127"),
    };
    let count: usize = match count_arg.parse::<usize>() {
        Ok(v) if v > 0 => v,
        _ => return usage(prog, "count must be a positive integer"),
    };

    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    for _ in 0..count {
        let id = SvidGenerator::generate(idtype, false);
        let human = id_to_human_readable(id);
        let d = DecomposedSvid::from_i64(id);
        if writeln!(
            out,
            "{}\t{}\tts={}\ttag={}\trand={}",
            id,
            human,
            d.unix_timestamp(),
            d.id_type,
            d.random,
        )
        .is_err()
        {
            return ExitCode::from(1);
        }
    }
    ExitCode::SUCCESS
}

fn usage(prog: &str, msg: &str) -> ExitCode {
    eprintln!("error: {msg}");
    eprintln!("usage: {prog} <idtype> <count>");
    eprintln!("  idtype   integer in 0..=127");
    eprintln!("  count    positive integer");
    ExitCode::from(2)
}
