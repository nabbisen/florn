//! The hand-written guests of `florn-test-guests`, plus what needs the Wasmtime adapter.
#![allow(dead_code)]

pub use florn_test_guests::*;

use florn_admission::Admission;
use florn_engine_wasmtime::{Limits, WasmtimeEngine};
use florn_host::{Options, Run, Seed};
use florn_platform::FixedPlatform;

pub fn engine() -> WasmtimeEngine {
    WasmtimeEngine::new(Limits::default()).unwrap()
}

pub fn options() -> Options {
    Options {
        keep_records: true,
        keep_payloads: true,
        chain_interval: 1_000,
        ..Options::default()
    }
}

pub fn new_run(spec: &ScenarioSpec, options: Options) -> Run<WasmtimeEngine> {
    Run::new(
        engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        spec.manifest(),
        &spec.modules(),
        Seed::Given(spec.seed),
        options,
    )
    .unwrap_or_else(|e| panic!("new run: {e}"))
}
