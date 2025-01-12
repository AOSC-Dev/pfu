//! Bash pattern matching.

use std::borrow::Cow;

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

impl ToString for GlobPart<'_> {
    fn to_string(&self) -> String {
        match self {
            GlobPart::String(text) => text.to_string(),
            GlobPart::Escaped(ch) => format!("\\{}", ch),
            GlobPart::AnyString => "*".to_string(),
            GlobPart::AnyChar => "?".to_string(),
            GlobPart::Range(range) => format!("[{}]", range),
            GlobPart::ZeroOrOneOf(list) => format!("?({})", list.to_string()),
            GlobPart::ZeroOrMoreOf(list) => format!("*({})", list.to_string()),
            GlobPart::OneOrMoreOf(list) => format!("+({})", list.to_string()),
            GlobPart::OneOf(list) => format!("@({})", list.to_string()),
            GlobPart::Not(list) => format!("!({})", list.to_string()),
        }
    }
}

/// A list of patterns.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PatternList<'a>(pub Vec<GlobPattern<'a>>);

impl ToString for PatternList<'_> {
    fn to_string(&self) -> String {
        self.0
            .iter()
            .map(GlobPattern::to_string)
            .collect::<Vec<_>>()
            .join("|")
    }
}

impl GlobPattern<'_> {
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
                    result.push_str("?");
                }
                GlobPart::ZeroOrMoreOf(list) => {
                    list.build_regex(result, greedy);
                    result.push_str("*");
                    result.push_str(lazy_flag);
                }
                GlobPart::OneOrMoreOf(list) => {
                    list.build_regex(result, greedy);
                    result.push_str("+");
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
    pub fn to_regex(&self, before: &str, after: &str, greedy: bool) -> super::eval::Result<Regex> {
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
fn pattern_list<'a>(i: &'a str) -> IResult<&'a str, PatternList<'a>> {
    map(
        many0(terminated(|i| glob_pattern(i, "|)"), opt(char('|')))),
        PatternList,
    )(i)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_glob_pattern() {
        let pat_list = PatternList(vec![
            GlobPattern(vec![GlobPart::String(Cow::Borrowed("a"))]),
            GlobPattern(vec![GlobPart::String(Cow::Borrowed("b"))]),
        ]);
        assert_eq!(
            glob_pattern("abc*?\\a[:ascii:]a?(a|b)*(a|b)+(a|b)@(a|b)!(a|b)}a", "}").unwrap(),
            (
                "}a",
                GlobPattern(vec![
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
        glob_pattern("abc*?\\aa?(a|b)*(a|b)+(a|b)@(a|b)!(a|b)}a", "}")
            .unwrap()
            .1
            .build_regex(&mut result, false);
        assert_eq!(result, "abc.*?.?aa(a|b)?(a|b)*?(a|b)+?(a|b)(?!(a|b)).*");

        let mut result = String::new();
        glob_pattern("abc*?\\aa?(a|b)*(a|b)+(a|b)@(a|b)!(a|b)}a", "}")
            .unwrap()
            .1
            .build_regex(&mut result, true);
        assert_eq!(result, "abc.*.?aa(a|b)?(a|b)*(a|b)+(a|b)(?!(a|b)).*");
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

    #[test]
    fn test_pattern_list() {
        assert_eq!(
            pattern_list("abc|LA?)").unwrap(),
            (
                ")",
                PatternList(vec![
                    GlobPattern(vec![GlobPart::String(Cow::Borrowed("abc")),]),
                    GlobPattern(vec![
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
