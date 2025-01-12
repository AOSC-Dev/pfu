//! Bash pattern matching.

use std::borrow::Cow;

use nom::{
    IResult,
    branch::alt,
    bytes::complete::{take_until1, take_while1},
    character::complete::{anychar, char},
    combinator::{map, value},
    multi::many1,
    sequence::{delimited, preceded},
};
use regex::{Regex, RegexBuilder};

/// A glob pattern, consisting of some [GlobPart]s.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GlobPattern<'a>(pub Vec<GlobPart<'a>>);

impl ToString for GlobPattern<'_> {
    fn to_string(&self) -> String {
        self.0
            .iter()
            .map(|part| part.to_string())
            .collect::<Vec<_>>()
            .join("")
    }
}

/// A element of glob pattern.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GlobPart<'a> {
    /// Matches a fixed string (`"<text>"`).
    String(Cow<'a, str>),
    /// Matches an escaped character (`"\\<char>"`).
    Escaped(char),
    /// Matches any string (`'*'`).
    AnyString,
    /// Matches any single character (`'?'`).
    AnyChar,
    /// Matches a characters range (`"[<range>]"`).
    Range(Cow<'a, str>),
}

impl ToString for GlobPart<'_> {
    fn to_string(&self) -> String {
        match self {
            GlobPart::String(text) => text.to_string(),
            GlobPart::Escaped(ch) => format!("\\{}", ch),
            GlobPart::AnyString => "*".to_string(),
            GlobPart::AnyChar => "?".to_string(),
            GlobPart::Range(range) => format!("[{}]", range),
        }
    }
}

impl GlobPattern<'_> {
    /// Converts a pattern into [Regex].
    pub fn to_regex(&self, before: &str, after: &str, greedy: bool) -> super::eval::Result<Regex> {
        let mut result = String::from(before);
        for part in &self.0 {
            match part {
                GlobPart::String(text) => result.push_str(&regex::escape(text.as_ref())),
                GlobPart::Escaped(ch) => result.push_str(&regex::escape(&ch.to_string())),
                GlobPart::AnyString => {
                    if greedy {
                        result.push_str(".*")
                    } else {
                        result.push_str(".*?")
                    }
                }
                GlobPart::AnyChar => result.push_str(".?"),
                GlobPart::Range(_) => todo!(),
            }
        }
        result.push_str(after);
        let result = RegexBuilder::new(&result)
            .case_insensitive(false)
            .multi_line(true)
            .unicode(true)
            .build()?;
        Ok(result)
    }
}

/// Parses a glob pattern.
#[inline]
pub fn glob_pattern<'a>(i: &'a str, exclude: &'static str) -> IResult<&'a str, GlobPattern<'a>> {
    map(many1(|s| glob_part(s, exclude)), |tokens| {
        GlobPattern(tokens)
    })(i)
}

#[inline]
fn glob_part<'a>(i: &'a str, exclude: &'static str) -> IResult<&'a str, GlobPart<'a>> {
    alt((
        // escaped
        map(preceded(char('\\'), anychar), GlobPart::Escaped),
        // any string
        value(GlobPart::AnyString, char('*')),
        // any char
        value(GlobPart::AnyChar, char('?')),
        // range
        map(delimited(char('['), take_until1("]"), char(']')), |range| {
            GlobPart::Range(Cow::Borrowed(range))
        }),
        // literal
        map(
            take_while1(|ch| !"[*?\\".contains(ch) && !exclude.contains(ch)),
            |s| GlobPart::String(Cow::Borrowed(s)),
        ),
    ))(i)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_glob_pattern() {
        assert_eq!(
            glob_pattern("abc*?\\a[:ascii:]a}a", "}").unwrap(),
            (
                "}a",
                GlobPattern(vec![
                    GlobPart::String(Cow::Borrowed("abc")),
                    GlobPart::AnyString,
                    GlobPart::AnyChar,
                    GlobPart::Escaped('a'),
                    GlobPart::Range(Cow::Borrowed(":ascii:")),
                    GlobPart::String(Cow::Borrowed("a")),
                ])
            )
        );
    }

    #[test]
    fn test_glob_part() {
        assert_eq!(
            glob_part("abc*", "").unwrap(),
            ("*", GlobPart::String(Cow::Borrowed("abc")))
        );
        assert_eq!(glob_part("*", "").unwrap(), ("", GlobPart::AnyString));
        assert_eq!(glob_part("?a", "").unwrap(), ("a", GlobPart::AnyChar));
        assert_eq!(
            glob_part("abcd", "c").unwrap(),
            ("cd", GlobPart::String(Cow::Borrowed("ab")))
        );
    }
}
