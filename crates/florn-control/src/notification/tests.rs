use super::*;

#[test]
fn encoding_is_canonical_and_json_is_one_line() {
    let a = Notification::new(
        "actor.faulted",
        Severity::Error,
        Scope::actor(3),
        Ends::Activation,
    )
    .with("actor", Value::Identity(3))
    .with("fault", Value::Code("fault.budget".into()))
    .with("metered", Value::Count(10_000_000))
    .with("policy", Value::Enum("drop".into()))
    .at(At {
        tick: 12,
        microstep: 1,
    });
    let mut b = a.clone();
    b.params.reverse();
    assert_eq!(
        a.encode(),
        b.encode(),
        "parameter order does not change the encoding"
    );
    let j = a.to_json();
    assert!(!j.contains('\n'));
    assert!(j.starts_with("{\"code\":\"actor.faulted\",\"severity\":\"error\",\"scope\":{\"run\":true,\"actor\":3},\"params\":{\"actor\":{\"type\":\"identity\",\"value\":3}"));
    assert!(j.ends_with("\"ends\":\"activation\",\"at\":{\"tick\":12,\"microstep\":1}}"));
    assert_eq!(json_str("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
}
