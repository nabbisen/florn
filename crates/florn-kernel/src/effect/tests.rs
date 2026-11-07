use super::*;

#[test]
fn quotas_and_contract() {
    let mut s = Sink::new(
        Quotas {
            effects_per_activation: 2,
            payload_bytes: 4,
            telemetry_per_activation_bytes: 5,
            pending_timers: 1,
        },
        vec![0],
        vec![1],
    );
    assert!(s.send(0, vec![1, 2, 3, 4]).is_ok());
    assert_eq!(
        s.send(1, vec![]).unwrap_err(),
        SinkRefusal::Contract {
            rule: "RFC 002 rule 23",
            detail: "port 1 is not a port of this actor".into()
        }
    );
    s.reset();
    assert_eq!(
        s.send(0, vec![0; 5]).unwrap_err(),
        SinkRefusal::Quota {
            quota: "payload",
            limit: 4
        }
    );
    s.reset();
    assert!(s.report(1, vec![0; 3]).is_ok());
    assert_eq!(
        s.report(0, vec![0; 3]).unwrap_err(),
        SinkRefusal::Quota {
            quota: "telemetry-per-activation",
            limit: 5
        }
    );
    s.reset();
    assert!(s.set_timer(0, 1).is_ok());
    assert!(s.cancel_timer(0).is_ok());
    assert_eq!(
        s.set_timer(0, 1).unwrap_err(),
        SinkRefusal::Quota {
            quota: "effects-per-activation",
            limit: 2
        }
    );
    assert_eq!(
        s.report(2, vec![]).unwrap_err(),
        SinkRefusal::Contract {
            rule: "RFC 002 rule 32",
            detail: "channel 2 is not declared".into()
        }
    );
}
