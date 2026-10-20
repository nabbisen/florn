//! The binary `florn`: the headless runner of RFC 005 rule 31 (`florn-cli`), the one entry point
//! a person installs (`cargo install --path crates/florn`).
#![forbid(unsafe_code)]

fn main() -> std::process::ExitCode {
    florn_cli::run(std::env::args().skip(1).collect())
}
