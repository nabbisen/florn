//! Fields a person types into (RFC 005 rule 39; external design §3.4): the text in a field
//! is the person's until it is committed, the model never writes into it while they type, and
//! it is set from the model only when the selection changes (`refresh` leaves a dirty field
//! alone). Problems a field can tell on its own are typed here; the Host's refusals arrive as
//! notifications and attach by their field path. No text of either is rendered here: the
//! shell's catalogue does that.
use florn_control::Notification;

/// What a field holds and expects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Any text, possibly empty.
    Text,
    /// A name: text that must not be empty.
    Name,
    /// An integer.
    Integer,
    /// A number without a unit, such as a ratio (`0.1`).
    Decimal,
    /// A quantity with its unit, one token (`5ms`, `1.2ohm`): a bare number is refused with
    /// the reason (RFC 004 rules 23 to 25).
    Quantity,
    /// `true` or `false`.
    Boolean,
}

/// A problem the field tells on its own, before the Host sees the value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    Empty,
    NotANumber,
    /// A quantity written as a bare number: the unit is missing.
    NeedsUnit,
    NotABoolean,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// The key the field edits: a config key, an attribute, `name`, `end`.
    pub name: String,
    pub kind: Kind,
    /// What is in the field: the person's text once they typed.
    pub text: String,
    /// The value the model holds.
    pub committed: String,
    /// Typed since the last commit or refresh.
    pub dirty: bool,
    pub problem: Option<Problem>,
    /// The Host's refusals that name this field, from the last validation.
    pub errors: Vec<Notification>,
    /// The unit or dimension the field expects, as the manifest declares it, for the label.
    pub unit: Option<String>,
}

impl Field {
    pub fn new(name: &str, kind: Kind, value: &str) -> Self {
        let mut f = Self {
            name: name.into(),
            kind,
            text: value.into(),
            committed: value.into(),
            dirty: false,
            problem: None,
            errors: Vec::new(),
            unit: None,
        };
        f.check();
        f
    }
    pub fn with_unit(mut self, unit: Option<String>) -> Self {
        self.unit = unit;
        self
    }
    /// The person typed: the text is theirs, checked at once, committed on `commit`.
    pub fn type_text(&mut self, text: &str) {
        self.text = text.into();
        self.dirty = true;
        self.check();
    }
    /// Commit what was typed; `true` when the value changed.
    pub fn commit(&mut self) -> bool {
        self.dirty = false;
        if self.problem.is_some() {
            return false;
        }
        if self.committed == self.text {
            return false;
        }
        self.committed = self.text.clone();
        true
    }
    /// The model's value changed: a field being typed into keeps its text (rule 39).
    pub fn refresh(&mut self, value: &str) {
        self.committed = value.into();
        if !self.dirty {
            self.text = value.into();
            self.check();
        }
    }
    /// Discard what was typed.
    pub fn revert(&mut self) {
        self.text = self.committed.clone();
        self.dirty = false;
        self.check();
    }
    pub fn is_valid(&self) -> bool {
        self.problem.is_none() && self.errors.is_empty()
    }
    fn check(&mut self) {
        let t = self.text.trim();
        self.problem = match self.kind {
            Kind::Text => None,
            Kind::Name => t.is_empty().then_some(Problem::Empty),
            Kind::Integer => {
                if t.is_empty() {
                    Some(Problem::Empty)
                } else if t.parse::<i64>().is_err() {
                    Some(Problem::NotANumber)
                } else {
                    None
                }
            }
            Kind::Decimal => {
                if t.is_empty() {
                    Some(Problem::Empty)
                } else if t.parse::<f64>().is_err()
                    || !t.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '.')
                {
                    Some(Problem::NotANumber)
                } else {
                    None
                }
            }
            Kind::Quantity => {
                if t.is_empty() {
                    Some(Problem::Empty)
                } else if t.parse::<f64>().is_ok() {
                    Some(Problem::NeedsUnit)
                } else if !t.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '.') {
                    Some(Problem::NotANumber)
                } else {
                    None
                }
            }
            Kind::Boolean => (t != "true" && t != "false").then_some(Problem::NotABoolean),
        };
    }
}

/// Attach the Host's refusals to the fields they name: a notification whose scope's field
/// path ends with `.<name>` or equals it goes to that field. Returns how many attached.
pub fn attach(fields: &mut [Field], prefix: &str, errors: &[Notification]) -> usize {
    let mut n = 0;
    for f in fields.iter_mut() {
        f.errors.clear();
        let own = format!("{prefix}.{}", f.name);
        for e in errors {
            if e.scope.field.as_deref() == Some(own.as_str()) {
                f.errors.push(e.clone());
                n += 1;
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
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
}
