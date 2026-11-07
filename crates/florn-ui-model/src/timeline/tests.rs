use super::*;

#[test]
fn entries_are_ordered_by_instant_and_filtered() {
    let mut t = Timeline::default();
    t.push_trace(
        0,
        &[
            TraceEntry::Handled {
                at: At {
                    tick: 5,
                    microstep: 0,
                },
                class: 2,
                origin: 1,
                seq: 1,
                target: 2,
                metered: 3,
                draws: 0,
                effects: 0,
                sends: Vec::new(),
            },
            TraceEntry::Fault {
                at: At {
                    tick: 7,
                    microstep: 0,
                },
                class: 1,
                origin: 3,
                seq: 1,
                actor: 3,
                code: "fault.budget".into(),
                parameter: 0,
                metered: 9,
            },
        ],
    );
    t.push_changes(&[ChangeSummary {
        at: 2,
        target: 100,
        payload: vec![("enabled".into(), "false".into())],
    }]);
    assert_eq!(t.visible(), vec![2, 0, 1]);
    assert_eq!(t.entries[2].detail, "enabled=false");
    t.filter.actor = Some(2);
    assert_eq!(t.visible(), vec![0]);
    t.filter = Filter {
        kinds: vec![Kind::Fault],
        ..Filter::default()
    };
    assert_eq!(t.visible(), vec![1]);
    assert_eq!(t.entries[1].record, Some(1));
    t.clear_run();
    assert_eq!(t.entries.len(), 1);
}
