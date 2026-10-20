//! `florn-conformance`: the kit's command.
//!
//! ```text
//! florn-conformance scenarios [--out <dir>]   write the shipped scenarios and their digests
//! florn-conformance figures [--quick]         run the benchmarks and print the figures
//! ```
#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("scenarios") => {
            let out = match args.iter().position(|a| a == "--out") {
                Some(i) => match args.get(i + 1) {
                    Some(d) => PathBuf::from(d),
                    None => return usage(),
                },
                None => florn_conformance::shipped::directory(),
            };
            match florn_conformance::shipped::write(&out) {
                Ok(d) => {
                    for (name, r) in &d.runs {
                        println!(
                            "{name}: seed {} chain {} records {} ended {}",
                            r.seed, r.chain, r.records, r.ended
                        );
                    }
                    println!("written to {}", out.display());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::from(1)
                }
            }
        }
        Some("figures") => {
            let quick = args.iter().any(|a| a == "--quick");
            let f = florn_conformance::bench::figures(quick);
            print!("{}", f.markdown());
            ExitCode::SUCCESS
        }
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!(
        "usage:\n  florn-conformance scenarios [--out <dir>]\n  florn-conformance figures [--quick]"
    );
    ExitCode::from(1)
}
