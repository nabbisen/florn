//! Quantities: one token, a decimal number followed by a unit (RFC 004 §4). Time is converted to
//! ticks with exact decimal arithmetic (rule 25); bytes with binary units (rule 26); every other
//! unit belongs to a domain (rule 27). A number is written back in its shortest form (RFC 001
//! rule 63): the canonical encoding writes the number text produced by [`Decimal::shortest`].

use std::fmt;

/// The tick length a scenario declares (RFC 001 rule 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TickLength {
    Ns,
    Ps,
    Fs,
}

impl TickLength {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "1ns" => Some(Self::Ns),
            "1ps" => Some(Self::Ps),
            "1fs" => Some(Self::Fs),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ns => "1ns",
            Self::Ps => "1ps",
            Self::Fs => "1fs",
        }
    }
    /// The tick length in femtoseconds, as a power of ten.
    /// The exponent of the tick's length in femtoseconds: `ns` is 6, `ps` 3, `fs` 0.
    pub fn femto_exponent(self) -> u32 {
        match self {
            Self::Ns => 6,
            Self::Ps => 3,
            Self::Fs => 0,
        }
    }
    pub fn as_quantity(self) -> (String, String) {
        ("1".into(), self.as_str()[1..].into())
    }
}

/// An exact decimal: `mantissa × 10^exponent`, with the mantissa free of trailing zeros when
/// normalised. Large enough for every duration a scenario can express (RFC 001 rule 2's longest run
/// at 1 fs is 5.1 hours = 1.8 × 10¹⁹ fs; u128 holds 3.4 × 10³⁸).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decimal {
    pub mantissa: u128,
    pub exponent: i32,
}

impl Decimal {
    /// Parse `123`, `0.5`, `1e-3`, `2.5E6`. No sign, no spaces, at least one digit.
    pub fn parse(text: &str) -> Option<Self> {
        let (num, exp) = match text.find(['e', 'E']) {
            Some(i) => (&text[..i], text[i + 1..].parse::<i32>().ok()?),
            None => (text, 0),
        };
        if num.is_empty() {
            return None;
        }
        let (int, frac) = match num.find('.') {
            Some(i) => (&num[..i], &num[i + 1..]),
            None => (num, ""),
        };
        if (int.is_empty() && frac.is_empty())
            || !int.bytes().all(|b| b.is_ascii_digit())
            || !frac.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let digits = format!("{int}{frac}");
        let mantissa: u128 = if digits.is_empty() {
            0
        } else {
            digits.parse().ok()?
        };
        let exponent = exp.checked_sub(i32::try_from(frac.len()).ok()?)?;
        Some(Self { mantissa, exponent }.normalised())
    }
    pub fn normalised(mut self) -> Self {
        if self.mantissa == 0 {
            self.exponent = 0;
            return self;
        }
        while self.mantissa.is_multiple_of(10) {
            self.mantissa /= 10;
            self.exponent += 1;
        }
        self
    }
    /// The value as an integer multiple of `10^unit_exponent`, if it is whole at that scale.
    pub fn as_integer_at(self, unit_exponent: i32) -> Option<u128> {
        let d = self.normalised();
        let shift = d.exponent - unit_exponent;
        if shift < 0 {
            return None; // digits below the unit: not whole
        }
        let mut v = d.mantissa;
        for _ in 0..shift {
            v = v.checked_mul(10)?;
        }
        Some(v)
    }
    /// Shortest decimal text that reads back as the same value: plain digits with a point where
    /// needed, never an exponent, no trailing zeros, no leading zeros but one before a point.
    pub fn shortest(self) -> String {
        let d = self.normalised();
        if d.mantissa == 0 {
            return "0".into();
        }
        let digits = d.mantissa.to_string();
        if d.exponent >= 0 {
            format!("{digits}{}", "0".repeat(d.exponent as usize))
        } else {
            let frac = (-d.exponent) as usize;
            if frac >= digits.len() {
                format!("0.{}{digits}", "0".repeat(frac - digits.len()))
            } else {
                let (a, b) = digits.split_at(digits.len() - frac);
                format!("{a}.{b}")
            }
        }
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.shortest())
    }
}

/// A quantity as parsed: the number and the unit text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quantity {
    pub number: Decimal,
    pub unit: String,
}

/// Why a token is not a quantity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuantityError {
    /// The token is a bare number (no unit).
    MissingUnit,
    /// The token is not `number unit`.
    Malformed,
}

impl Quantity {
    /// Parse `5ms`, `0.5mH`, `1e-3s`, `24V`, `0.01USD`. A bare number is [`QuantityError::MissingUnit`].
    pub fn parse(token: &str) -> Result<Self, QuantityError> {
        let token = token.trim();
        let split = token
            .find(|c: char| {
                !(c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '-' || c == '+')
            })
            .unwrap_or(token.len());
        // An exponent sign belongs to the number; a unit starts at the first other character.
        let mut split = split;
        // `1e-3s`: the scan above stops at 's'. `5ms`: stops at 'm'. `24V`: stops at 'V'. `1E6`: whole token is number.
        // But `e`/`E` may also start a unit (e.g. `3eV`): if the char after an 'e' is not a digit or sign, the 'e' begins the unit.
        let bytes = token.as_bytes();
        let mut i = 0;
        while i < split {
            if bytes[i] == b'e' || bytes[i] == b'E' {
                let next = bytes.get(i + 1).copied();
                let is_exp = matches!(next, Some(b'0'..=b'9' | b'-' | b'+'));
                if !is_exp {
                    split = i;
                    break;
                }
            }
            i += 1;
        }
        let (num, unit) = token.split_at(split);
        if num.contains(['-', '+']) && !num.contains(['e', 'E']) {
            return Err(QuantityError::Malformed);
        }
        let number = Decimal::parse(num).ok_or(QuantityError::Malformed)?;
        if unit.is_empty() {
            return Err(QuantityError::MissingUnit);
        }
        if !unit
            .chars()
            .all(|c| c.is_alphanumeric() || c == 'µ' || c == '/' || c == '%')
        {
            return Err(QuantityError::Malformed);
        }
        Ok(Self {
            number,
            unit: unit.to_string(),
        })
    }

    pub fn as_pair(&self) -> (String, String) {
        (self.number.shortest(), self.unit.clone())
    }

    /// The time units the core knows (rule 25), as the exponent of the unit in femtoseconds.
    pub fn time_unit_femto_exponent(unit: &str) -> Option<u32> {
        match unit {
            "s" => Some(15),
            "ms" => Some(12),
            "us" | "µs" => Some(9),
            "ns" => Some(6),
            "ps" => Some(3),
            "fs" => Some(0),
            _ => None,
        }
    }

    /// Convert a time quantity to ticks (rule 25). `Err(None)` when the unit is not a time unit;
    /// `Err(Some(smallest))` when the value is not a whole number of ticks, naming the smallest tick
    /// length that would make it whole (or `Fs` if none does); `Ok(None)` when the value does not
    /// fit 64 bits of ticks.
    pub fn to_ticks(&self, tick: TickLength) -> Result<Option<u64>, Option<TickLength>> {
        let unit_exp = Self::time_unit_femto_exponent(&self.unit).ok_or(None)?;
        // value in femtoseconds = number × 10^(unit_exp); ticks = value_fs / 10^(tick_exp)
        let femto = Decimal {
            mantissa: self.number.mantissa,
            exponent: self.number.exponent + unit_exp as i32,
        };
        match femto.as_integer_at(tick.femto_exponent() as i32) {
            Some(t) => Ok(u64::try_from(t).ok()),
            None => {
                let smallest = [TickLength::Ns, TickLength::Ps, TickLength::Fs]
                    .into_iter()
                    .find(|t| femto.as_integer_at(t.femto_exponent() as i32).is_some());
                Err(Some(smallest.unwrap_or(TickLength::Fs)))
            }
        }
    }

    /// Convert a byte quantity (rule 26): `B`, `KiB`, `MiB`. `Err(false)` when the unit is not a
    /// byte unit; `Err(true)` when the value is not a whole number of bytes.
    pub fn to_bytes(&self) -> Result<Option<u64>, bool> {
        let factor: u128 = match self.unit.as_str() {
            "B" => 1,
            "KiB" => 1024,
            "MiB" => 1024 * 1024,
            "GiB" => 1024 * 1024 * 1024,
            _ => return Err(false),
        };
        let whole = self.number.as_integer_at(0).ok_or(true)?;
        Ok(whole
            .checked_mul(factor)
            .and_then(|b| u64::try_from(b).ok()))
    }
}

impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.number, self.unit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals() {
        assert_eq!(
            Decimal::parse("5").unwrap(),
            Decimal {
                mantissa: 5,
                exponent: 0
            }
        );
        assert_eq!(
            Decimal::parse("0.5").unwrap(),
            Decimal {
                mantissa: 5,
                exponent: -1
            }
        );
        assert_eq!(
            Decimal::parse("1e-3").unwrap(),
            Decimal {
                mantissa: 1,
                exponent: -3
            }
        );
        assert_eq!(
            Decimal::parse("2.50E6").unwrap(),
            Decimal {
                mantissa: 25,
                exponent: 5
            }
        );
        assert_eq!(Decimal::parse("1.5").unwrap().shortest(), "1.5");
        assert_eq!(Decimal::parse("0.001").unwrap().shortest(), "0.001");
        assert_eq!(Decimal::parse("2.50E6").unwrap().shortest(), "2500000");
        assert_eq!(Decimal::parse("000").unwrap().shortest(), "0");
        assert!(Decimal::parse("").is_none());
        assert!(Decimal::parse(".").is_none());
        assert!(Decimal::parse("1.2.3").is_none());
    }

    #[test]
    fn quantities_and_ticks() {
        let q = Quantity::parse("5ms").unwrap();
        assert_eq!(q.to_ticks(TickLength::Ns), Ok(Some(5_000_000)));
        assert_eq!(
            Quantity::parse("1.5ns").unwrap().to_ticks(TickLength::Ns),
            Err(Some(TickLength::Ps))
        );
        assert_eq!(
            Quantity::parse("1.0000005ns")
                .unwrap()
                .to_ticks(TickLength::Ns),
            Err(Some(TickLength::Fs))
        );
        assert_eq!(
            Quantity::parse("60s").unwrap().to_ticks(TickLength::Ns),
            Ok(Some(60_000_000_000))
        );
        assert_eq!(
            Quantity::parse("1e-3s").unwrap().to_ticks(TickLength::Ps),
            Ok(Some(1_000_000_000))
        );
        assert_eq!(
            Quantity::parse("600y").unwrap().to_ticks(TickLength::Ns),
            Err(None)
        );
        assert_eq!(
            Quantity::parse("5").unwrap_err(),
            QuantityError::MissingUnit
        );
        assert_eq!(Quantity::parse("ms").unwrap_err(), QuantityError::Malformed);
        assert_eq!(
            Quantity::parse("4MiB").unwrap().to_bytes(),
            Ok(Some(4 * 1024 * 1024))
        );
        assert_eq!(Quantity::parse("0.5B").unwrap().to_bytes(), Err(true));
        assert_eq!(Quantity::parse("3eV").unwrap().unit, "eV");
        assert_eq!(
            Quantity::parse("1.2ohm").unwrap().as_pair(),
            ("1.2".to_string(), "ohm".to_string())
        );
        assert_eq!(
            Quantity::parse("0.01USD").unwrap().as_pair(),
            ("0.01".to_string(), "USD".to_string())
        );
    }
}
