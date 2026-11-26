//! RFC 012 rule 9: the order of breakpoints, their replacement at one tick, and what the plant sees.

use super::*;

fn bp(tick: u64, input: Input, value: f64) -> Breakpoint {
    Breakpoint {
        tick,
        input,
        value,
        origin: 1,
    }
}

#[test]
fn a_breakpoint_replaces_a_pending_one_for_the_same_input_at_the_same_tick() {
    let mut p = Pending::default();
    p.record(bp(10, Input::Signal(0), 1.0));
    p.record(bp(10, Input::Signal(1), 2.0));
    p.record(bp(10, Input::Signal(0), 3.0));
    // One for signal 0 and one for signal 1, in the order of the last handling.
    assert_eq!(
        p.items()
            .iter()
            .map(|b| (b.input, b.value))
            .collect::<Vec<_>>(),
        [(Input::Signal(1), 2.0), (Input::Signal(0), 3.0)]
    );
    // At another tick the earlier one stays.
    p.record(bp(20, Input::Signal(0), 4.0));
    assert_eq!(p.len(), 3);
}

#[test]
fn the_value_an_input_will_have_is_the_last_pending_one() {
    let mut p = Pending::default();
    assert_eq!(p.last_value(Input::Source(3)), None);
    p.record(bp(5, Input::Source(3), 1.0));
    p.record(bp(9, Input::Source(3), 2.0));
    assert_eq!(p.last_value(Input::Source(3)), Some(2.0));
}

#[test]
fn the_next_breakpoint_after_the_solved_tick_and_the_origins() {
    let mut p = Pending::default();
    p.record(Breakpoint {
        origin: 7,
        ..bp(30, Input::Signal(0), 1.0)
    });
    p.record(Breakpoint {
        origin: 3,
        ..bp(20, Input::Signal(1), 1.0)
    });
    p.record(Breakpoint {
        origin: 7,
        ..bp(10, Input::Signal(2), 1.0)
    });
    assert_eq!(p.next_after(10), Some(20));
    assert_eq!(p.next_after(30), None);
    assert_eq!(p.origins(), [3, 7]);
}
