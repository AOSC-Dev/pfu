//! Parser to convert string source to AST.

use std::{borrow::Cow, rc::Rc};

use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{tag, take_till, take_until1, take_while, take_while1},
    character::complete::{anychar, char, newline},
    combinator::{map, map_res, opt, value},
    multi::{many0, many1},
    sequence::{delimited, pair, preceded, separated_pair},
};

use super::ast::*;

pub fn apml_ast(i: &str) -> IResult<&str, ApmlAst> {
    map(many0(token), |tokens| ApmlAst(tokens))(i)
}

#[inline]
pub fn token(i: &str) -> IResult<&str, Token> {
    alt((
        // space
        value(Token::Space, char(' ')),
        // newline
        value(Token::Newline, newline),
        // comment
        comment_token,
        // variable definition
        variable_def.map(|def| Token::Variable(def)),
    ))(i)
}

#[inline]
pub fn comment_token(i: &str) -> IResult<&str, Token> {
    map(preceded(char('#'), take_till(|ch| ch == '\n')), |comment| {
        Token::Comment(Cow::Borrowed(comment))
    })(i)
}

#[inline]
pub fn variable_def(i: &str) -> IResult<&str, VariableDefinition> {
    map(
        separated_pair(variable_name, char('='), variable_value),
        |(name, value)| VariableDefinition {
            name: Cow::Borrowed(name),
            value,
        },
    )(i)
}

#[inline]
pub fn variable_name(i: &str) -> IResult<&str, &str> {
    take_while1(|ch: char| ch.is_alphanumeric() || ch == '_')(i)
}

#[inline]
pub fn variable_value(i: &str) -> IResult<&str, VariableValue> {
    alt((
        // string
        map(text, |text| VariableValue::String(Rc::new(text))),
    ))(i)
}

#[inline]
pub fn text(i: &str) -> IResult<&str, Text> {
    map(many0(text_unit), Text)(i)
}

#[inline]
pub fn text_unit(i: &str) -> IResult<&str, TextUnit> {
    alt((
        // single quoted
        delimited(
            char('\''),
            map(take_while(|ch| ch != '\''), |s| {
                TextUnit::SingleQuote(Cow::Borrowed(s))
            }),
            char('\''),
        ),
        // double quoted
        delimited(
            char('"'),
            map(many0(|s| word(s, true)), TextUnit::DuobleQuote),
            char('"'),
        ),
        // unquoted
        map(many1(|s| word(s, false)), TextUnit::Unquoted),
    ))(i)
}

#[inline]
pub fn word(i: &str, accept_space: bool) -> IResult<&str, Word> {
    alt((
        // braced variable
        map(delimited(tag("${"), braced_expansion, char('}')), |exp| {
            Word::BracedVariable(exp)
        }),
        // unbraced variable
        map(preceded(char('$'), variable_name), |name| {
            Word::UnbracedVariable(Cow::Borrowed(name))
        }),
        // literal
        map(many1(|s| literal_part(s, accept_space)), |parts| {
            Word::Literal(parts)
        }),
    ))(i)
}

#[inline]
pub fn literal_part(i: &str, double_quoted: bool) -> IResult<&str, LiteralPart> {
    alt((
        // line continuation
        value(LiteralPart::LineContinuation, tag("\\\n")),
        // escaped
        map(preceded(char('\\'), anychar), LiteralPart::Escaped),
        // literal
        map(
            take_while1(|ch| {
                !"$'\"\\\n".contains(ch) && (double_quoted || (ch != ' ' && ch != '#'))
            }),
            |s| LiteralPart::String(Cow::Borrowed(s)),
        ),
    ))(i)
}

#[inline]
pub fn braced_expansion(i: &str) -> IResult<&str, BracedExpansion> {
    alt((
        // length of
        map(preceded(char('#'), variable_name), |name| BracedExpansion {
            name: Cow::Borrowed(name),
            modifier: Some(ExpansionModifier::Length),
        }),
        // other
        map(
            pair(variable_name, opt(expansion_modifier)),
            |(name, modifier)| BracedExpansion {
                name: Cow::Borrowed(name),
                modifier,
            },
        ),
    ))(i)
}

#[inline]
pub fn expansion_modifier(i: &str) -> IResult<&str, ExpansionModifier> {
    alt((substring_expansion_modifier,))(i)
}

#[inline]
pub fn substring_expansion_modifier(i: &str) -> IResult<&str, ExpansionModifier> {
    preceded(
        char(':'),
        map(
            pair(
                map_res(take_while1(|ch: char| ch.is_digit(10)), |num| {
                    usize::from_str_radix(num, 10)
                }),
                opt(preceded(
                    char(':'),
                    map_res(take_while1(|ch: char| ch.is_digit(10)), |num| {
                        usize::from_str_radix(num, 10)
                    }),
                )),
            ),
            |(offset, length)| ExpansionModifier::Substring { offset, length },
        ),
    )(i)
}

#[inline]
pub fn glob_pattern<'a>(i: &'a str, exclude: &'static str) -> IResult<&'a str, GlobPattern<'a>> {
    map(many1(|s| glob_part(s, exclude)), |tokens| {
        GlobPattern(tokens)
    })(i)
}

#[inline]
pub fn glob_part<'a>(i: &'a str, exclude: &'static str) -> IResult<&'a str, GlobPart<'a>> {
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
    use crate::apml::parser::*;

    #[test]
    fn test_ast() {
        let src = r##"# Test APML

a=b # Inline comment
K="${#a} $ab b\ #l \
c"
"##;
        assert_eq!(
            apml_ast(src).unwrap(),
            (
                "",
                ApmlAst(vec![
                    Token::Comment(Cow::Borrowed(" Test APML")),
                    Token::Newline,
                    Token::Newline,
                    Token::Variable(VariableDefinition {
                        name: Cow::Borrowed("a"),
                        value: VariableValue::String(Rc::new(Text(vec![TextUnit::Unquoted(
                            vec![Word::Literal(vec![LiteralPart::String(Cow::Borrowed("b"))])]
                        )])))
                    }),
                    Token::Space,
                    Token::Comment(Cow::Borrowed(" Inline comment")),
                    Token::Newline,
                    Token::Variable(VariableDefinition {
                        name: Cow::Borrowed("K"),
                        value: VariableValue::String(Rc::new(Text(vec![TextUnit::DuobleQuote(
                            vec![
                                Word::BracedVariable(BracedExpansion {
                                    name: Cow::Borrowed("a"),
                                    modifier: Some(ExpansionModifier::Length)
                                }),
                                Word::Literal(vec![LiteralPart::String(Cow::Borrowed(" ")),]),
                                Word::UnbracedVariable(Cow::Borrowed("ab")),
                                Word::Literal(vec![
                                    LiteralPart::String(Cow::Borrowed(" b")),
                                    LiteralPart::Escaped(' '),
                                    LiteralPart::String(Cow::Borrowed("#l ")),
                                    LiteralPart::LineContinuation,
                                    LiteralPart::String(Cow::Borrowed("c")),
                                ])
                            ]
                        )])))
                    }),
                    Token::Newline
                ])
            )
        );
        assert_eq!(apml_ast(src).unwrap().1.to_string(), src);
    }

    #[test]
    fn test_token() {
        assert_eq!(
            token("#asdf").unwrap(),
            ("", Token::Comment(Cow::Borrowed("asdf")))
        );
        assert_eq!(
            token("#asdf \n").unwrap(),
            ("\n", Token::Comment(Cow::Borrowed("asdf ")))
        );
        assert_eq!(
            token("#\n").unwrap(),
            ("\n", Token::Comment(Cow::Borrowed("")))
        );
        assert_eq!(token(" ").unwrap(), ("", Token::Space));
        assert_eq!(token("\n").unwrap(), ("", Token::Newline));
        assert_eq!(
            token("a=\n").unwrap(),
            (
                "\n",
                Token::Variable(VariableDefinition {
                    name: Cow::Borrowed("a"),
                    value: VariableValue::String(Rc::new(Text(vec![])))
                })
            )
        );
    }

    #[test]
    fn test_variable_def() {
        variable_def("=\n").unwrap_err();
        variable_def("?=\n").unwrap_err();
        assert_eq!(
            variable_def("a=\n").unwrap(),
            ("\n", VariableDefinition {
                name: Cow::Borrowed("a"),
                value: VariableValue::String(Rc::new(Text(vec![])))
            })
        );
        assert_eq!(
            variable_def("a=b$0\n").unwrap(),
            ("\n", VariableDefinition {
                name: Cow::Borrowed("a"),
                value: VariableValue::String(Rc::new(Text(vec![TextUnit::Unquoted(vec![
                    Word::Literal(vec![LiteralPart::String(Cow::Borrowed("b"))]),
                    Word::UnbracedVariable(Cow::Borrowed("0")),
                ])])))
            })
        );
    }

    #[test]
    fn test_variable_name() {
        assert_eq!(variable_name("123a").unwrap(), ("", "123a"));
        assert_eq!(variable_name("a!!!").unwrap(), ("!!!", "a"));
        variable_name("!!!").unwrap_err();
        variable_name("").unwrap_err();
    }

    #[test]
    fn test_variable_value() {
        assert_eq!(
            variable_value("\n").unwrap(),
            ("\n", VariableValue::String(Rc::new(Text(vec![]))))
        );
        assert_eq!(
            variable_value("123\\n\\\na!!@$1#").unwrap(),
            (
                "#",
                VariableValue::String(Rc::new(Text(vec![TextUnit::Unquoted(vec![
                    Word::Literal(vec![
                        LiteralPart::String(Cow::Borrowed("123")),
                        LiteralPart::Escaped('n'),
                        LiteralPart::LineContinuation,
                        LiteralPart::String(Cow::Borrowed("a!!@")),
                    ]),
                    Word::UnbracedVariable(Cow::Borrowed("1")),
                ])])))
            )
        );
        assert_eq!(
            variable_value("\"${#a} b\\ #l \\\nc\"\n").unwrap(),
            (
                "\n",
                VariableValue::String(Rc::new(Text(vec![TextUnit::DuobleQuote(vec![
                    Word::BracedVariable(BracedExpansion {
                        name: Cow::Borrowed("a"),
                        modifier: Some(ExpansionModifier::Length)
                    }),
                    Word::Literal(vec![
                        LiteralPart::String(Cow::Borrowed(" b")),
                        LiteralPart::Escaped(' '),
                        LiteralPart::String(Cow::Borrowed("#l ")),
                        LiteralPart::LineContinuation,
                        LiteralPart::String(Cow::Borrowed("c"))
                    ])
                ])])))
            )
        );
    }

    #[test]
    fn test_text() {
        assert_eq!(text("").unwrap(), ("", Text(vec![])));
        assert_eq!(
            text("asd\\f\\\n134$a'test'\"a$a${a}  \" a").unwrap(),
            (
                " a",
                Text(vec![
                    TextUnit::Unquoted(vec![
                        Word::Literal(vec![
                            LiteralPart::String(Cow::Borrowed("asd")),
                            LiteralPart::Escaped('f'),
                            LiteralPart::LineContinuation,
                            LiteralPart::String(Cow::Borrowed("134"))
                        ]),
                        Word::UnbracedVariable(Cow::Borrowed("a")),
                    ]),
                    TextUnit::SingleQuote(Cow::Borrowed("test")),
                    TextUnit::DuobleQuote(vec![
                        Word::Literal(vec![LiteralPart::String(Cow::Borrowed("a"))]),
                        Word::UnbracedVariable(Cow::Borrowed("a")),
                        Word::BracedVariable(BracedExpansion {
                            name: Cow::Borrowed("a"),
                            modifier: None
                        }),
                        Word::Literal(vec![LiteralPart::String(Cow::Borrowed("  "))]),
                    ])
                ])
            )
        );
        assert_eq!(
            text("asd\\f\n134$a'test'\"a$a${a}  \" a").unwrap(),
            (
                "\n134$a'test'\"a$a${a}  \" a",
                Text(vec![TextUnit::Unquoted(vec![Word::Literal(vec![
                    LiteralPart::String(Cow::Borrowed("asd")),
                    LiteralPart::Escaped('f'),
                ]),]),])
            )
        );
    }

    #[test]
    fn test_text_unit() {
        assert_eq!(
            text_unit("asdf134 a").unwrap(),
            (
                " a",
                TextUnit::Unquoted(vec![Word::Literal(vec![LiteralPart::String(
                    Cow::Borrowed("asdf134")
                )])])
            )
        );
        assert_eq!(
            text_unit("'123 a'").unwrap(),
            ("", TextUnit::SingleQuote(Cow::Borrowed("123 a")))
        );
        assert_eq!(
            text_unit("1$a${#b} a$a").unwrap(),
            (
                " a$a",
                TextUnit::Unquoted(vec![
                    Word::Literal(vec![LiteralPart::String(Cow::Borrowed("1"))]),
                    Word::UnbracedVariable(Cow::Borrowed("a")),
                    Word::BracedVariable(BracedExpansion {
                        name: Cow::Borrowed("b"),
                        modifier: Some(ExpansionModifier::Length),
                    }),
                ])
            )
        );
        assert_eq!(
            text_unit("\"1\\\na$a${#b}\" a").unwrap(),
            (
                " a",
                TextUnit::DuobleQuote(vec![
                    Word::Literal(vec![
                        LiteralPart::String(Cow::Borrowed("1")),
                        LiteralPart::LineContinuation,
                        LiteralPart::String(Cow::Borrowed("a"))
                    ]),
                    Word::UnbracedVariable(Cow::Borrowed("a")),
                    Word::BracedVariable(BracedExpansion {
                        name: Cow::Borrowed("b"),
                        modifier: Some(ExpansionModifier::Length),
                    }),
                ])
            )
        );
        text_unit("").unwrap_err();
    }

    #[test]
    fn test_word() {
        assert_eq!(
            word("asdf134 a", false).unwrap(),
            (
                " a",
                Word::Literal(vec![LiteralPart::String(Cow::Borrowed("asdf134"))])
            )
        );
        assert_eq!(
            word("asdf134 a", true).unwrap(),
            (
                "",
                Word::Literal(vec![LiteralPart::String(Cow::Borrowed("asdf134 a"))])
            )
        );
        assert_eq!(
            word("asdf\\134\\\n a", true).unwrap(),
            (
                "",
                Word::Literal(vec![
                    LiteralPart::String(Cow::Borrowed("asdf")),
                    LiteralPart::Escaped('1'),
                    LiteralPart::String(Cow::Borrowed("34")),
                    LiteralPart::LineContinuation,
                    LiteralPart::String(Cow::Borrowed(" a")),
                ])
            )
        );
        assert_eq!(
            word("$123 a", true).unwrap(),
            (" a", Word::UnbracedVariable(Cow::Borrowed("123")))
        );
        assert_eq!(
            word("${abc} a", true).unwrap(),
            (
                " a",
                Word::BracedVariable(BracedExpansion {
                    name: Cow::Borrowed("abc"),
                    modifier: None
                })
            )
        );
        assert_eq!(
            word("${#abc} a", true).unwrap(),
            (
                " a",
                Word::BracedVariable(BracedExpansion {
                    name: Cow::Borrowed("abc"),
                    modifier: Some(ExpansionModifier::Length)
                })
            )
        );
        word("${#abc:1} a", true).unwrap_err();
        word("", true).unwrap_err();
        assert_eq!(
            word("${abc:1:2} a", true).unwrap(),
            (
                " a",
                Word::BracedVariable(BracedExpansion {
                    name: Cow::Borrowed("abc"),
                    modifier: Some(ExpansionModifier::Substring {
                        offset: 1,
                        length: Some(2)
                    })
                })
            )
        );
    }

    #[test]
    fn test_literal_part() {
        assert_eq!(
            literal_part("abc a", false).unwrap(),
            (" a", LiteralPart::String(Cow::Borrowed("abc")))
        );
        assert_eq!(
            literal_part("abc a", true).unwrap(),
            ("", LiteralPart::String(Cow::Borrowed("abc a")))
        );
        assert_eq!(
            literal_part("\\na a", false).unwrap(),
            ("a a", LiteralPart::Escaped('n'))
        );
        assert_eq!(
            literal_part("\\\na a", false).unwrap(),
            ("a a", LiteralPart::LineContinuation)
        );
    }

    #[test]
    fn test_braced_expansion() {
        assert_eq!(
            braced_expansion("asdf134").unwrap(),
            ("", BracedExpansion {
                name: Cow::Borrowed("asdf134"),
                modifier: None
            })
        );
        assert_eq!(
            braced_expansion("asdf:10").unwrap(),
            ("", BracedExpansion {
                name: Cow::Borrowed("asdf"),
                modifier: Some(ExpansionModifier::Substring {
                    offset: 10,
                    length: None
                })
            })
        );
        assert_eq!(
            braced_expansion("#1").unwrap(),
            ("", BracedExpansion {
                name: Cow::Borrowed("1"),
                modifier: Some(ExpansionModifier::Length)
            })
        );
    }

    #[test]
    fn test_expansion_modifier() {
        assert_eq!(
            expansion_modifier(":10").unwrap(),
            ("", ExpansionModifier::Substring {
                offset: 10,
                length: None
            })
        );
        assert_eq!(
            expansion_modifier(":10:1").unwrap(),
            ("", ExpansionModifier::Substring {
                offset: 10,
                length: Some(1)
            })
        );
        expansion_modifier(":").unwrap_err();
        expansion_modifier("1").unwrap_err();
    }

    #[test]
    fn test_substring_expansion_modifier() {
        assert_eq!(
            substring_expansion_modifier(":10").unwrap(),
            ("", ExpansionModifier::Substring {
                offset: 10,
                length: None
            })
        );
        assert_eq!(
            substring_expansion_modifier(":10:1").unwrap(),
            ("", ExpansionModifier::Substring {
                offset: 10,
                length: Some(1)
            })
        );
        substring_expansion_modifier(":").unwrap_err();
        substring_expansion_modifier("1").unwrap_err();
    }

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
