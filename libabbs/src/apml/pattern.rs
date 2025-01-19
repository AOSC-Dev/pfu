//! Bash pattern matching used in APML.

use std::{
    borrow::Cow,
    fmt::{Display, Write},
};

use nom::{
    IResult,
    branch::alt,
    bytes::complete::{tag, take_until1, take_while1},
    character::complete::{anychar, char},
    combinator::{map, opt, value},
    multi::{many0, many1},
    sequence::{delimited, preceded, terminated},
};
use regex::{Regex, RegexBuilder};

/// A pattern, consisting of one or more [`GlobPart`]s.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BashPattern<'a>(pub Vec<GlobPart<'a>>);

impl Display for BashPattern<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for part in &self.0 {
            Display::fmt(part, f)?;
        }
        Ok(())
    }
}

/// A element of [pattern][BashPattern].
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
    /// Matches zero or one occurrence of some patterns (`"?(<PATTERNS>)"`).
    ZeroOrOneOf(PatternList<'a>),
    /// Matches zero or more occurrence of some patterns (`"*(<PATTERNS>)"`).
    ZeroOrMoreOf(PatternList<'a>),
    /// Matches one or more occurrence of some patterns (`"+(<PATTERNS>)"`).
    OneOrMoreOf(PatternList<'a>),
    /// Matches one of some patterns (`"@(<PATTERNS>)"`).
    OneOf(PatternList<'a>),
    /// Matches anything except of some patterns (`"!(<PATTERNS>)"`).
    Not(PatternList<'a>),
}

impl Display for GlobPart<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GlobPart::String(text) => f.write_str(text),
            GlobPart::Escaped(ch) => {
                f.write_char('\\')?;
                f.write_char(*ch)?;
                Ok(())
            }
            GlobPart::AnyString => f.write_char('*'),
            GlobPart::AnyChar => f.write_char('?'),
            GlobPart::Range(range) => f.write_fmt(format_args!("[{}]", range)),
            GlobPart::ZeroOrOneOf(list) => f.write_fmt(format_args!("?({})", list)),
            GlobPart::ZeroOrMoreOf(list) => f.write_fmt(format_args!("*({})", list)),
            GlobPart::OneOrMoreOf(list) => f.write_fmt(format_args!("+({})", list)),
            GlobPart::OneOf(list) => f.write_fmt(format_args!("@({})", list)),
            GlobPart::Not(list) => f.write_fmt(format_args!("!({})", list)),
        }
    }
}

/// A list of patterns.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PatternList<'a>(pub Vec<BashPattern<'a>>);

impl Display for PatternList<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (idx, pattern) in (1..).zip(&self.0) {
            if idx != 1 {
                f.write_char('|')?;
            }
            Display::fmt(pattern, f)?;
        }
        Ok(())
    }
}

impl BashPattern<'_> {
    /// Converts a pattern into regex string.
    pub fn build_regex(&self, result: &mut String, greedy: bool) {
        let lazy_flag = if greedy { "" } else { "?" };
        for part in &self.0 {
            match part {
                GlobPart::String(text) => result.push_str(&regex::escape(text.as_ref())),
                GlobPart::Escaped(ch) => result.push_str(&regex::escape(&ch.to_string())),
                GlobPart::AnyString => {
                    result.push_str(".*");
                    result.push_str(lazy_flag);
                }
                GlobPart::AnyChar => result.push_str(".?"),
                GlobPart::Range(_) => todo!(),
                GlobPart::ZeroOrOneOf(list) => {
                    list.build_regex(result, greedy);
                    result.push('?');
                }
                GlobPart::ZeroOrMoreOf(list) => {
                    list.build_regex(result, greedy);
                    result.push('*');
                    result.push_str(lazy_flag);
                }
                GlobPart::OneOrMoreOf(list) => {
                    list.build_regex(result, greedy);
                    result.push('+');
                    result.push_str(lazy_flag);
                }
                GlobPart::OneOf(list) => {
                    list.build_regex(result, greedy);
                }
                GlobPart::Not(list) => {
                    result.push_str("(?!");
                    list.build_regex(result, greedy);
                    // always greedy
                    result.push_str(").*");
                }
            }
        }
    }

    /// Converts a pattern into [Regex].
    pub fn to_regex(&self, before: &str, after: &str, greedy: bool) -> Result<Regex, regex::Error> {
        let mut result = String::from(before);
        self.build_regex(&mut result, greedy);
        result.push_str(after);
        let result = RegexBuilder::new(&result)
            .case_insensitive(false)
            .multi_line(true)
            .unicode(true)
            .build()?;
        Ok(result)
    }
}

impl PatternList<'_> {
    /// Converts a pattern list into regex string.
    pub fn build_regex(&self, result: &mut String, greedy: bool) {
        result.push('(');
        for (pattern, idx) in self.0.iter().zip(1..) {
            if idx != 1 {
                result.push('|');
            }
            pattern.build_regex(result, greedy);
        }
        result.push(')');
    }
}

/// Parses a glob pattern.
#[inline]
pub fn bash_pattern<'a>(i: &'a str, exclude: &'static str) -> IResult<&'a str, BashPattern<'a>> {
    map(many1(|s| pattern_part(s, exclude)), |tokens| {
        BashPattern(tokens)
    })(i)
}

#[inline]
fn pattern_part<'a>(i: &'a str, exclude: &'static str) -> IResult<&'a str, GlobPart<'a>> {
    alt((
        // escaped
        map(preceded(char('\\'), anychar), GlobPart::Escaped),
        // zero or one of
        map(
            delimited(tag("?("), pattern_list, char(')')),
            GlobPart::ZeroOrOneOf,
        ),
        // zero or more of
        map(
            delimited(tag("*("), pattern_list, char(')')),
            GlobPart::ZeroOrMoreOf,
        ),
        // one or more of
        map(
            delimited(tag("+("), pattern_list, char(')')),
            GlobPart::OneOrMoreOf,
        ),
        // one of
        map(
            delimited(tag("@("), pattern_list, char(')')),
            GlobPart::OneOf,
        ),
        // anything except
        map(delimited(tag("!("), pattern_list, char(')')), GlobPart::Not),
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

#[inline]
fn pattern_list(i: &str) -> IResult<&str, PatternList> {
    map(
        many0(terminated(|i| bash_pattern(i, "|)"), opt(char('|')))),
        PatternList,
    )(i)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_bash_pattern() {
        let pat_list = PatternList(vec![
            BashPattern(vec![GlobPart::String(Cow::Borrowed("a"))]),
            BashPattern(vec![GlobPart::String(Cow::Borrowed("b"))]),
        ]);
        assert_eq!(
            bash_pattern("abc*?\\a[:ascii:]a?(a|b)*(a|b)+(a|b)@(a|b)!(a|b)}a", "}").unwrap(),
            (
                "}a",
                BashPattern(vec![
                    GlobPart::String(Cow::Borrowed("abc")),
                    GlobPart::AnyString,
                    GlobPart::AnyChar,
                    GlobPart::Escaped('a'),
                    GlobPart::Range(Cow::Borrowed(":ascii:")),
                    GlobPart::String(Cow::Borrowed("a")),
                    GlobPart::ZeroOrOneOf(pat_list.clone()),
                    GlobPart::ZeroOrMoreOf(pat_list.clone()),
                    GlobPart::OneOrMoreOf(pat_list.clone()),
                    GlobPart::OneOf(pat_list.clone()),
                    GlobPart::Not(pat_list.clone()),
                ])
            )
        );

        let mut result = String::new();
        bash_pattern("abc*?\\aa?(a|b)*(a|b)+(a|b)@(a|b)!(a|b)}a", "}")
            .unwrap()
            .1
            .build_regex(&mut result, false);
        assert_eq!(result, "abc.*?.?aa(a|b)?(a|b)*?(a|b)+?(a|b)(?!(a|b)).*");

        let mut result = String::new();
        bash_pattern("abc*?\\aa?(a|b)*(a|b)+(a|b)@(a|b)!(a|b)}a", "}")
            .unwrap()
            .1
            .build_regex(&mut result, true);
        assert_eq!(result, "abc.*.?aa(a|b)?(a|b)*(a|b)+(a|b)(?!(a|b)).*");
    }

    #[test]
    fn test_pattern_part() {
        assert_eq!(
            pattern_part("abc*", "").unwrap(),
            ("*", GlobPart::String(Cow::Borrowed("abc")))
        );
        assert_eq!(pattern_part("*", "").unwrap(), ("", GlobPart::AnyString));
        assert_eq!(pattern_part("?a", "").unwrap(), ("a", GlobPart::AnyChar));
        assert_eq!(
            pattern_part("abcd", "c").unwrap(),
            ("cd", GlobPart::String(Cow::Borrowed("ab")))
        );
    }

    #[test]
    fn test_pattern_list() {
        assert_eq!(
            pattern_list("abc|LA?)").unwrap(),
            (
                ")",
                PatternList(vec![
                    BashPattern(vec![GlobPart::String(Cow::Borrowed("abc")),]),
                    BashPattern(vec![
                        GlobPart::String(Cow::Borrowed("LA")),
                        GlobPart::AnyChar,
                    ]),
                ])
            )
        );
        assert_eq!(pattern_list("abc|LA?)").unwrap().1.to_string(), "abc|LA?");

        let mut result = String::new();
        pattern_list("abc|LA?)")
            .unwrap()
            .1
            .build_regex(&mut result, false);
        assert_eq!(result, "(abc|LA.?)");
    }
}
