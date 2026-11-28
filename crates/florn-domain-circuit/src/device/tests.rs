//! RFC 012 rules 19 and 20: the world, and the calls into the device with what the solver checks
//! before it uses a result, over a scripted call-in handle.

use super::*;

/// What a scripted handle was asked, and what it answers next.
#[derive(Default)]
struct Script {
    asked: Vec<String>,
    faults: Vec<(FaultKind, u32)>,
    /// The result of the next `call`.
    result: Option<Result<Option<Val>, CallError>>,
    /// The error of the next `open`, `commit` or `discard`.
    error: Option<CallError>,
    bound: Option<u32>,
    args: Vec<Vec<Val>>,
}

impl CallIn for Script {
    fn open(&mut self, actor: Identity, export: &str, args: &[Val]) -> Result<(), CallError> {
        self.asked.push(format!("open {actor} {export}"));
        self.args.push(args.to_vec());
        self.error.take().map_or(Ok(()), Err)
    }
    fn call(
        &mut self,
        export: &str,
        args: &[Val],
        list_bound: Option<u32>,
    ) -> Result<Option<Val>, CallError> {
        self.asked.push(format!("call {export}"));
        self.args.push(args.to_vec());
        self.bound = list_bound;
        self.result.take().expect("a scripted result")
    }
    fn commit(&mut self, export: &str, args: &[Val]) -> Result<(), CallError> {
        self.asked.push(format!("commit {export}"));
        self.args.push(args.to_vec());
        self.error.take().map_or(Ok(()), Err)
    }
    fn discard(&mut self, export: &str, args: &[Val]) -> Result<(), CallError> {
        self.asked.push(format!("discard {export}"));
        self.args.push(args.to_vec());
        self.error.take().map_or(Ok(()), Err)
    }
    fn fault(&mut self, kind: FaultKind, parameter: u32) -> CallError {
        self.faults.push((kind, parameter));
        CallError::Faulted { kind, parameter }
    }
    fn run_ended(&self) -> bool {
        false
    }
}

impl Script {
    /// A handle whose next `call` answers `result`.
    fn answering(result: Result<Option<Val>, CallError>) -> Self {
        Self {
            result: Some(result),
            ..Self::default()
        }
    }
    /// A handle whose next `open`, `commit` or `discard` fails with `error`.
    fn failing(error: CallError) -> Self {
        Self {
            error: Some(error),
            ..Self::default()
        }
    }
}

fn list(values: &[f64]) -> Option<Val> {
    Some(Val::List(values.iter().map(|v| Val::F64(*v)).collect()))
}

/// A two-pin device's answer: two currents, four Jacobian entries, the flag.
fn good() -> Vec<f64> {
    vec![0.5, -0.5, 1.0, -1.0, -1.0, 1.0, 0.0]
}

#[test]
fn the_world_is_the_device_of_rule_19_with_its_compound_activation() {
    let w = world();
    assert_eq!(w.name, "florn:circuit/device@0.1.0");
    assert_eq!(w.exports_ids, ["florn:circuit/device-model@0.1.0"]);
    assert!(w.imports.is_empty() && w.imports_ids.is_empty() && w.records.is_empty());
    let (id, functions) = &w.exports[0];
    assert_eq!(id, "florn:circuit/device-model@0.1.0");
    let names: Vec<&str> = functions.iter().map(|f| f.name).collect();
    assert_eq!(
        names,
        ["begin-step", "evaluate", "accept-step", "reject-step"]
    );
    // `begin-step(start: tick, h: f64)`, `evaluate(list<f64>) -> list<f64>`.
    assert_eq!(functions[0].params.len(), 2);
    assert_eq!(functions[1].params.len(), 1);
    assert!(functions[1].result.is_some() && functions[3].result.is_none());
    // The compound: `begin-step`, then `evaluate` any number of times, then either close.
    let c = &w.compounds[0];
    assert!(c.has(BEGIN_STEP) && c.has(EVALUATE) && c.has(ACCEPT_STEP) && c.has(REJECT_STEP));
    assert_eq!(c.open, BEGIN_STEP);
    assert_eq!(c.between, [EVALUATE]);
    assert_eq!(c.commit, ACCEPT_STEP);
    assert_eq!(c.discard, REJECT_STEP);
}

#[test]
fn the_calls_carry_the_step_the_trial_and_the_solution() {
    let mut s = Script::answering(Ok(list(&good())));
    let mut d = Calls::new(&mut s, 7, 2);
    d.begin(1_500, 2.5e-6).unwrap();
    let stamp = d.evaluate(&[0.6, 0.0]).unwrap();
    d.accept(&[0.61, 0.0]).unwrap();
    assert_eq!(stamp.currents, [0.5, -0.5]);
    assert_eq!(stamp.jacobian, [1.0, -1.0, -1.0, 1.0]);
    assert!(!stamp.limited);
    assert_eq!(
        s.asked,
        [
            format!("open 7 {BEGIN_STEP}"),
            format!("call {EVALUATE}"),
            format!("commit {ACCEPT_STEP}"),
        ]
    );
    assert_eq!(s.args[0], [Val::U64(1_500), Val::F64(2.5e-6)]);
    assert_eq!(s.args[1], [Val::List(vec![Val::F64(0.6), Val::F64(0.0)])]);
    assert_eq!(s.args[2], [Val::List(vec![Val::F64(0.61), Val::F64(0.0)])]);
    // The bound the handle checks before it lifts the list is n + n² + 1, inclusive.
    assert_eq!(s.bound, Some(7));
}

#[test]
fn a_rejected_step_is_closed_by_the_discarding_call() {
    let mut s = Script::default();
    let mut d = Calls::new(&mut s, 7, 2);
    d.begin(0, 0.0).unwrap();
    d.reject().unwrap();
    assert_eq!(s.asked[1], format!("discard {REJECT_STEP}"));
    assert!(s.faults.is_empty());
}

#[test]
fn the_flag_is_one_zero_or_the_result_is_invalid() {
    for (flag, limited) in [(1.0, true), (0.0, false)] {
        let mut s = Script::default();
        let mut answer = good();
        answer[6] = flag;
        s.result = Some(Ok(list(&answer)));
        let stamp = Calls::new(&mut s, 7, 2).evaluate(&[0.0, 0.0]).unwrap();
        assert_eq!(stamp.limited, limited);
    }
    for flag in [2.0, 0.5, -1.0] {
        let mut s = Script::default();
        let mut answer = good();
        answer[6] = flag;
        s.result = Some(Ok(list(&answer)));
        let e = Calls::new(&mut s, 7, 2).evaluate(&[0.0, 0.0]).unwrap_err();
        assert!(
            matches!(e, Lost::Faulted { parameter: 12_020, invalid: Some(r), .. } if r.contains("neither 0 nor 1")),
            "{e:?}"
        );
        assert_eq!(s.faults, [(FaultKind::Contract, 12_020)]);
    }
}

#[test]
fn a_result_of_the_wrong_length_or_with_a_value_that_is_not_finite_is_a_contract_violation() {
    for (answer, why) in [
        (vec![0.5, -0.5, 1.0], "n + n² + 1"),
        (vec![], "n + n² + 1"),
        (vec![0.5, f64::NAN, 1.0, -1.0, -1.0, 1.0, 0.0], "not finite"),
        (
            vec![0.5, -0.5, f64::INFINITY, -1.0, -1.0, 1.0, 0.0],
            "not finite",
        ),
        (
            vec![0.5, -0.5, 1.0, -1.0, -1.0, 1.0, f64::NAN],
            "not finite",
        ),
    ] {
        let mut s = Script::answering(Ok(list(&answer)));
        let e = Calls::new(&mut s, 7, 2).evaluate(&[0.0, 0.0]).unwrap_err();
        match e {
            Lost::Faulted {
                kind,
                parameter,
                invalid: Some(reason),
            } => {
                assert_eq!((kind, parameter), (FaultKind::Contract, 12_020));
                assert!(reason.contains(why), "{reason} for {answer:?}");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(s.faults, [(FaultKind::Contract, 12_020)], "{answer:?}");
    }
}

#[test]
fn a_result_that_is_no_list_of_numbers_is_invalid() {
    for result in [
        Ok(None),
        Ok(Some(Val::F64(1.0))),
        Ok(Some(Val::List(vec![Val::U32(1)]))),
    ] {
        let mut s = Script::answering(result);
        let e = Calls::new(&mut s, 7, 2).evaluate(&[0.0, 0.0]).unwrap_err();
        assert!(
            matches!(
                e,
                Lost::Faulted {
                    parameter: 12_020,
                    invalid: Some(_),
                    ..
                }
            ),
            "{e:?}"
        );
    }
}

#[test]
fn a_result_over_the_bound_is_the_same_fault_and_is_never_lifted() {
    let mut s = Script::answering(Err(CallError::ResultTooLong { len: 8, bound: 7 }));
    let e = Calls::new(&mut s, 7, 2).evaluate(&[0.0, 0.0]).unwrap_err();
    assert!(
        matches!(e, Lost::Faulted { parameter: 12_020, invalid: Some(r), .. } if r.contains("longer")),
        "{e:?}"
    );
    assert_eq!(s.faults, [(FaultKind::Contract, 12_020)]);
}

#[test]
fn a_fault_of_the_guest_is_lost_as_it_was_and_adds_no_fault_of_ours() {
    for (kind, parameter) in [
        (FaultKind::Trap, 0),
        (FaultKind::Budget, 0),
        (FaultKind::Contract, 12_019),
    ] {
        let mut s = Script::answering(Err(CallError::Faulted { kind, parameter }));
        let e = Calls::new(&mut s, 7, 2).evaluate(&[0.0, 0.0]).unwrap_err();
        assert_eq!(
            e,
            Lost::Faulted {
                kind,
                parameter,
                invalid: None
            }
        );
        assert!(s.faults.is_empty());
    }
    // The same in the opening and the closing calls.
    let mut s = Script::failing(CallError::Faulted {
        kind: FaultKind::Trap,
        parameter: 0,
    });
    assert!(matches!(
        Calls::new(&mut s, 7, 2).begin(0, 1e-9),
        Err(Lost::Faulted {
            kind: FaultKind::Trap,
            ..
        })
    ));
    let mut s = Script::failing(CallError::Faulted {
        kind: FaultKind::Budget,
        parameter: 0,
    });
    assert!(matches!(
        Calls::new(&mut s, 7, 2).accept(&[0.0, 0.0]),
        Err(Lost::Faulted {
            kind: FaultKind::Budget,
            ..
        })
    ));
}

#[test]
fn a_run_that_ended_and_a_refused_call_are_told_apart() {
    let mut s = Script::failing(CallError::RunEnded);
    assert_eq!(Calls::new(&mut s, 7, 2).begin(0, 1e-9), Err(Lost::Ended));
    let mut s = Script::failing(CallError::Refused("the actor is dead"));
    assert_eq!(Calls::new(&mut s, 7, 2).begin(0, 1e-9), Err(Lost::Gone));
    let mut s = Script::answering(Err(CallError::RunEnded));
    assert_eq!(
        Calls::new(&mut s, 7, 2).evaluate(&[0.0, 0.0]),
        Err(Lost::Ended)
    );
}

#[test]
fn no_device_is_absent_and_closes_without_a_call() {
    let mut n = NoDevice;
    assert!(!n.present());
    assert!(n.begin(0, 1.0).is_ok() && n.accept(&[]).is_ok() && n.reject().is_ok());
}

#[test]
fn the_contract_faults_have_the_numbers_of_rules_19_and_20() {
    // The RFC's number times 1,000 plus the rule's (RFC 012 rule 20).
    assert_eq!(SEND_IN_DEVICE, 12_019);
    assert_eq!(STAMP_INVALID, 12_020);
}
