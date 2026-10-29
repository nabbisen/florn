//! The module validator and transformation (RFC 003 sections 2 and 3): any bytes as a module.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Both cost-model versions (RFC 010 rule 7): the transformation under each.
    let mut admission = florn_admission::Admission::default();
    for version in florn_admission::COST_MODEL_VERSIONS {
        let _ = admission.admit_under(
            "fuzz",
            data,
            &florn_admission::ClassLimits::defaults(),
            version,
        );
    }
});
