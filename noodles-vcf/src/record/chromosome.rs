//! VCF record chromosome.

use std::{error, fmt, str::FromStr};

use super::MISSING_FIELD;

/// A VCF record chromosome (`CHROM`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Chromosome {
    /// A reference sequence name.
    Name(String),
    /// A symbol.
    Symbol(String),
}

impl fmt::Display for Chromosome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name(name) => f.write_str(name),
            Self::Symbol(symbol) => write!(f, "<{symbol}>"),
        }
    }
}

/// An error returned when a raw VCF record chromosome fails to parse.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseError {
    /// The input is empty.
    Empty,
    /// The input is missing.
    Missing,
    /// The input is invalid.
    Invalid,
}

impl error::Error for ParseError {}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("empty input"),
            Self::Missing => f.write_str("missing input"),
            Self::Invalid => f.write_str("invalid input"),
        }
    }
}

impl FromStr for Chromosome {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(ParseError::Empty);
        } else if s == MISSING_FIELD {
            return Err(ParseError::Missing);
        }

        // symbol
        if let Some(t) = s.strip_prefix('<') {
            if let Some(t) = t.strip_suffix('>') {
                return Ok(Self::Symbol(t.into()));
            }
        }

        // name
        if is_valid_name(s) {
            Ok(Self::Name(s.into()))
        } else {
            Err(ParseError::Invalid)
        }
    }
}

// § 1.4.7 Contig field format
fn is_valid_name_char(c: char) -> bool {
    ('!'..='~').contains(&c)
        && !matches!(
            c,
            '\\' | ',' | '"' | '`' | '\'' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>',
        )
}

pub(crate) fn is_valid_name(s: &str) -> bool {
    let mut chars = s.chars();

    if let Some(c) = chars.next() {
        if c == '*' || c == '=' || !is_valid_name_char(c) {
            return false;
        }
    }

    chars.all(is_valid_name_char)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fmt() {
        assert_eq!(Chromosome::Name(String::from("sq0")).to_string(), "sq0");
        assert_eq!(Chromosome::Symbol(String::from("sq0")).to_string(), "<sq0>");
    }

    #[test]
    fn test_from_str() {
        assert_eq!("sq0".parse(), Ok(Chromosome::Name(String::from("sq0"))));
        assert_eq!("<sq0>".parse(), Ok(Chromosome::Symbol(String::from("sq0"))));

        assert_eq!("".parse::<Chromosome>(), Err(ParseError::Empty));
        assert_eq!(".".parse::<Chromosome>(), Err(ParseError::Missing));
        assert_eq!("sq 0".parse::<Chromosome>(), Err(ParseError::Invalid));
        assert_eq!("sq[0]".parse::<Chromosome>(), Err(ParseError::Invalid));
        assert_eq!(">sq0".parse::<Chromosome>(), Err(ParseError::Invalid));
        assert_eq!("*sq0".parse::<Chromosome>(), Err(ParseError::Invalid));
        assert_eq!("=sq0".parse::<Chromosome>(), Err(ParseError::Invalid));
    }
}
