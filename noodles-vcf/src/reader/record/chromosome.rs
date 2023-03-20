use std::io;

use crate::record::Chromosome;

pub(super) fn parse_chromosome(s: &str, chromosome: &mut Chromosome) -> io::Result<()> {
    // symbol
    if let Some(t) = s.strip_prefix('<') {
        if let Some(t) = t.strip_suffix('>') {
            if !matches!(chromosome, Chromosome::Symbol(symbol) if symbol == t) {
                *chromosome = Chromosome::Symbol(t.into());
            }

            return Ok(());
        }
    }

    // name
    if !matches!(chromosome, Chromosome::Name(name) if name == s) {
        if is_valid_name(s) {
            *chromosome = Chromosome::Name(s.into());
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid chromosome",
            ));
        }
    }

    Ok(())
}

// § 1.4.7 "Contig field format"
fn is_valid_name_char(c: char) -> bool {
    ('!'..='~').contains(&c)
        && !matches!(
            c,
            '\\' | ',' | '"' | '`' | '\'' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>',
        )
}

fn is_valid_name(s: &str) -> bool {
    let mut chars = s.chars();

    let is_valid_first_char = chars
        .next()
        .map(|c| c != '*' && c != '=' && is_valid_name_char(c))
        .unwrap_or_default();

    is_valid_first_char && chars.all(is_valid_name_char)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_chromosome() -> io::Result<()> {
        let mut chromosome = Chromosome::Name(String::from("."));

        parse_chromosome("sq0", &mut chromosome)?;
        assert_eq!(chromosome, Chromosome::Name(String::from("sq0")));

        parse_chromosome("<sq0>", &mut chromosome)?;
        assert_eq!(chromosome, Chromosome::Symbol(String::from("sq0")));

        assert!(matches!(
            parse_chromosome("", &mut chromosome),
            Err(e) if e.kind() == io::ErrorKind::InvalidData,
        ));

        Ok(())
    }

    #[test]
    fn test_is_valid_name() {
        assert!(is_valid_name("sq0"));

        assert!(!is_valid_name(""));
        assert!(!is_valid_name("sq 0"));
        assert!(!is_valid_name("sq[0]"));
        assert!(!is_valid_name(">sq0"));
        assert!(!is_valid_name("*sq0"));
        assert!(!is_valid_name("=sq0"));
    }
}