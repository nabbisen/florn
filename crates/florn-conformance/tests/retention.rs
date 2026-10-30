//! DATA-008 (RFC 009 rule 9, the browser Host): a run that keeps only the records since the
//! last checkpoint answers a window before them by replay, and an export's trace is the whole
//! run's, assembled; the digest and the kept checkpoints are those of a run keeping every record.
use florn_conformance::host::{self, options};
use florn_conformance::shipped;
use florn_host::{Advance, Options};

#[test]
fn the_records_since_the_last_checkpoint_serve_every_window_and_the_whole_export() {
    let s = shipped::ospf_lab();
    let mut full = host::try_start(
        &s,
        Options {
            checkpoint_interval: 100,
            checkpoint_memory: u64::MAX,
            ..options()
        },
    )
    .unwrap();
    full.run_to_end();
    let all = full.records().unwrap().to_vec();

    let mut r = host::try_start(
        &s,
        Options {
            checkpoint_interval: 100,
            checkpoint_memory: u64::MAX,
            records_since_checkpoint: true,
            ..options()
        },
    )
    .unwrap();
    r.advance(Advance::ToTick(2_000));
    let kept = r.records().unwrap().len();
    let total = r.kernel().trace.records() as usize;
    assert!(kept < total && kept <= 100 + 50, "kept {kept} of {total}");
    assert_eq!(
        r.trace_window(total as u64 - 10, 10),
        all[total - 10..total].to_vec()
    );
    assert_eq!(r.trace_window(0, 50), all[..50].to_vec(), "by replay");
    assert_eq!(
        r.trace_window(total as u64 - kept as u64 - 20, 40),
        all[total - kept - 20..total - kept + 20].to_vec(),
        "across the boundary"
    );
    r.run_to_end();
    assert_eq!(r.chain(), full.chain());
    assert_eq!(r.kept().len(), full.kept().len());
    assert!(r.records().unwrap().len() < all.len());
    assert_eq!(
        r.full_trace().unwrap(),
        all,
        "the export's trace is the whole run's"
    );
    assert_eq!(full.full_trace().unwrap(), all);
}
