use super::*;

#[test]
fn typed_text_stays_until_committed_and_survives_a_refresh() {
    let mut f = Field::new("delay", Kind::Quantity, "2ns");
    f.type_text("5m");
    assert!(f.dirty && f.problem.is_none());
    f.refresh("3ns");
    assert_eq!(
        f.text, "5m",
        "the model never writes into a field being typed into"
    );
    assert_eq!(f.committed, "3ns");
    f.type_text("5ms");
    assert!(f.commit());
    assert_eq!(f.committed, "5ms");
    assert!(!f.dirty);
    f.refresh("7ms");
    assert_eq!(f.text, "7ms");
}

#[test]
fn a_bare_number_is_no_quantity() {
    let mut f = Field::new("delay", Kind::Quantity, "2ns");
    f.type_text("5");
    assert_eq!(f.problem, Some(Problem::NeedsUnit));
    assert!(!f.commit());
    assert_eq!(f.committed, "2ns");
    f.type_text("");
    assert_eq!(f.problem, Some(Problem::Empty));
    f.type_text("fast");
    assert_eq!(f.problem, Some(Problem::NotANumber));
    f.revert();
    assert_eq!(f.text, "2ns");
    assert!(f.is_valid());
    let mut i = Field::new("infinity", Kind::Integer, "16");
    i.type_text("many");
    assert_eq!(i.problem, Some(Problem::NotANumber));
    let mut b = Field::new("enabled", Kind::Boolean, "true");
    b.type_text("yes");
    assert_eq!(b.problem, Some(Problem::NotABoolean));
}
