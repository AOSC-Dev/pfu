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

use super::tree::*;

pub fn apml_ast(i: &str) -> IResult<&str, ApmlAst> {
    map(many0(token), |tokens| ApmlAst(tokens))(i)
}

#[inline]
pub fn token(i: &str) -> IResult<&str, Token> {
    alt((
        // spacy
        map(alt((char(' '), char('\t'))), Token::Spacy),
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
        map(
            |s| text(s, " #"),
            |text| VariableValue::String(Rc::new(text)),
        ),
    ))(i)
}

#[inline]
pub fn text<'a>(i: &'a str, exclude: &'static str) -> IResult<&'a str, Text<'a>> {
    map(many0(|s| text_unit(s, exclude)), Text)(i)
}

#[inline]
pub fn text_unit<'a>(i: &'a str, exclude: &'static str) -> IResult<&'a str, TextUnit<'a>> {
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
            map(many0(|s| word(s, "")), TextUnit::DuobleQuote),
            char('"'),
        ),
        // unquoted
        map(many1(|s| word(s, exclude)), TextUnit::Unquoted),
    ))(i)
}

#[inline]
pub fn word<'a>(i: &'a str, exclude: &'static str) -> IResult<&'a str, Word<'a>> {
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
        map(many1(|s| literal_part(s, exclude)), |parts| {
            Word::Literal(parts)
        }),
    ))(i)
}

#[inline]
pub fn literal_part<'a>(i: &'a str, exclude: &'static str) -> IResult<&'a str, LiteralPart<'a>> {
    alt((
        // line continuation
        value(LiteralPart::LineContinuation, tag("\\\n")),
        // escaped
        map(preceded(char('\\'), anychar), LiteralPart::Escaped),
        // literal
        map(
            take_while1(|ch| !"$'\"\\\n".contains(ch) && !exclude.contains(ch)),
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
    #[inline]
    fn expansion_glob(i: &str) -> IResult<&str, Rc<GlobPattern>> {
        map(|s| glob_pattern(s, "}"), Rc::new)(i)
    }
    #[inline]
    fn expansion_glob_replace(i: &str) -> IResult<&str, Rc<GlobPattern>> {
        map(|s| glob_pattern(s, "}/"), Rc::new)(i)
    }
    #[inline]
    fn expansion_text(i: &str) -> IResult<&str, Rc<Text>> {
        map(|s| text(s, "}"), Rc::new)(i)
    }
    alt((
        substring_expansion_modifier,
        map(
            preceded(tag("##"), expansion_glob),
            ExpansionModifier::StripLongestPrefix,
        ),
        map(
            preceded(char('#'), expansion_glob),
            ExpansionModifier::StripShortestPrefix,
        ),
        map(
            preceded(tag("%%"), expansion_glob),
            ExpansionModifier::StripLongestSuffix,
        ),
        map(
            preceded(char('%'), expansion_glob),
            ExpansionModifier::StripShortestSuffix,
        ),
        map(
            preceded(
                tag("//"),
                separated_pair(expansion_glob_replace, char('/'), expansion_text),
            ),
            |(pattern, string)| ExpansionModifier::ReplaceAll { pattern, string },
        ),
        map(
            preceded(
                tag("/#"),
                separated_pair(expansion_glob_replace, char('/'), expansion_text),
            ),
            |(pattern, string)| ExpansionModifier::ReplacePrefix { pattern, string },
        ),
        map(
            preceded(
                tag("/%"),
                separated_pair(expansion_glob_replace, char('/'), expansion_text),
            ),
            |(pattern, string)| ExpansionModifier::ReplaceSuffix { pattern, string },
        ),
        map(
            preceded(
                char('/'),
                separated_pair(expansion_glob_replace, char('/'), expansion_text),
            ),
            |(pattern, string)| ExpansionModifier::ReplaceOnce { pattern, string },
        ),
        map(
            preceded(tag("^^"), expansion_glob),
            ExpansionModifier::UpperAll,
        ),
        map(
            preceded(char('^'), expansion_glob),
            ExpansionModifier::UpperOnce,
        ),
        map(
            preceded(tag(",,"), expansion_glob),
            ExpansionModifier::LowerAll,
        ),
        map(
            preceded(char(','), expansion_glob),
            ExpansionModifier::LowerOnce,
        ),
        map(
            preceded(tag("^^"), expansion_glob),
            ExpansionModifier::UpperAll,
        ),
        map(
            preceded(tag(":?"), expansion_text),
            ExpansionModifier::ErrorOnUnset,
        ),
        map(
            preceded(tag(":-"), expansion_text),
            ExpansionModifier::WhenUnset,
        ),
        map(
            preceded(tag(":+"), expansion_text),
            ExpansionModifier::WhenSet,
        ),
    ))(i)
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
K=a"${#a} $ab b\ #l \
c ${1:1}${1:1:1}${1##a}${1#a.*[:alpha:]b?\?}${1%%1}${1%1}\
${1/a/a}${1//a?a/$a}${1/#a/b}${1/%a/b}${1^*}${1^^*}${1,*}\
${1,,*}${1:?err}${1:-unset}${1:+set}"
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
                    Token::Spacy(' '),
                    Token::Comment(Cow::Borrowed(" Inline comment")),
                    Token::Newline,
                    Token::Variable(VariableDefinition {
                        name: Cow::Borrowed("K"),
                        value: VariableValue::String(Rc::new(Text(vec![
                            TextUnit::Unquoted(vec![Word::Literal(vec![LiteralPart::String(
                                Cow::Borrowed("a")
                            )])]),
                            TextUnit::DuobleQuote(vec![
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
                                    LiteralPart::String(Cow::Borrowed("c ")),
                                ]),
                                Word::BracedVariable(BracedExpansion {
                                    name: Cow::Borrowed("1"),
                                    modifier: Some(ExpansionModifier::Substring {
                                        offset: 1,
                                        length: None
                                    })
                                }),
                                Word::BracedVariable(BracedExpansion {
                                    name: Cow::Borrowed("1"),
                                    modifier: Some(ExpansionModifier::Substring {
                                        offset: 1,
                                        length: Some(1)
                                    })
                                }),
                                Word::BracedVariable(BracedExpansion {
                                    name: Cow::Borrowed("1"),
                                    modifier: Some(ExpansionModifier::StripLongestPrefix(Rc::new(
                                        GlobPattern(vec![GlobPart::String(Cow::Borrowed("a"))])
                                    )))
                                }),
                                Word::BracedVariable(BracedExpansion {
                                    name: Cow::Borrowed("1"),
                                    modifier: Some(ExpansionModifier::StripShortestPrefix(
                                        Rc::new(GlobPattern(vec![
                                            GlobPart::String(Cow::Borrowed("a.")),
                                            GlobPart::AnyString,
                                            GlobPart::Range(Cow::Borrowed(":alpha:")),
                                            GlobPart::String(Cow::Borrowed("b")),
                                            GlobPart::AnyChar,
                                            GlobPart::Escaped('?'),
                                        ]))
                                    ))
                                }),
                                Word::BracedVariable(BracedExpansion {
                                    name: Cow::Borrowed("1"),
                                    modifier: Some(ExpansionModifier::StripLongestSuffix(Rc::new(
                                        GlobPattern(vec![GlobPart::String(Cow::Borrowed("1"))])
                                    )))
                                }),
                                Word::BracedVariable(BracedExpansion {
                                    name: Cow::Borrowed("1"),
                                    modifier: Some(ExpansionModifier::StripShortestSuffix(
                                        Rc::new(GlobPattern(vec![GlobPart::String(
                                            Cow::Borrowed("1")
                                        )]))
                                    ))
                                }),
                                Word::Literal(vec![LiteralPart::LineContinuation]),
                                Word::BracedVariable(BracedExpansion {
                                    name: Cow::Borrowed("1"),
                                    modifier: Some(ExpansionModifier::ReplaceOnce {
                                        pattern: Rc::new(GlobPattern(vec![GlobPart::String(
                                            Cow::Borrowed("a")
                                        )])),
                                        string: Rc::new(Text(vec![TextUnit::Unquoted(vec![
                                            Word::Literal(vec![LiteralPart::String(
                                                Cow::Borrowed("a")
                                            )])
                                        ])]))
                                    })
                                }),
                                Word::BracedVariable(BracedExpansion {
                                    name: Cow::Borrowed("1"),
                                    modifier: Some(ExpansionModifier::ReplaceAll {
                                        pattern: Rc::new(GlobPattern(vec![
                                            GlobPart::String(Cow::Borrowed("a")),
                                            GlobPart::AnyChar,
                                            GlobPart::String(Cow::Borrowed("a"))
                                        ])),
                                        string: Rc::new(Text(vec![TextUnit::Unquoted(vec![
                                            Word::UnbracedVariable(Cow::Borrowed("a"))
                                        ])]))
                                    })
                                }),
                                Word::BracedVariable(BracedExpansion {
                                    name: Cow::Borrowed("1"),
                                    modifier: Some(ExpansionModifier::ReplacePrefix {
                                        pattern: Rc::new(GlobPattern(vec![GlobPart::String(
                                            Cow::Borrowed("a")
                                        )])),
                                        string: Rc::new(Text(vec![TextUnit::Unquoted(vec![
                                            Word::Literal(vec![LiteralPart::String(
                                                Cow::Borrowed("b")
                                            )])
                                        ])]))
                                    })
                                }),
                                Word::BracedVariable(BracedExpansion {
                                    name: Cow::Borrowed("1"),
                                    modifier: Some(ExpansionModifier::ReplaceSuffix {
                                        pattern: Rc::new(GlobPattern(vec![GlobPart::String(
                                            Cow::Borrowed("a")
                                        )])),
                                        string: Rc::new(Text(vec![TextUnit::Unquoted(vec![
                                            Word::Literal(vec![LiteralPart::String(
                                                Cow::Borrowed("b")
                                            )])
                                        ])]))
                                    })
                                }),
                                Word::BracedVariable(BracedExpansion {
                                    name: Cow::Borrowed("1"),
                                    modifier: Some(ExpansionModifier::UpperOnce(Rc::new(
                                        GlobPattern(vec![GlobPart::AnyString])
                                    )))
                                }),
                                Word::BracedVariable(BracedExpansion {
                                    name: Cow::Borrowed("1"),
                                    modifier: Some(ExpansionModifier::UpperAll(Rc::new(
                                        GlobPattern(vec![GlobPart::AnyString])
                                    )))
                                }),
                                Word::BracedVariable(BracedExpansion {
                                    name: Cow::Borrowed("1"),
                                    modifier: Some(ExpansionModifier::LowerOnce(Rc::new(
                                        GlobPattern(vec![GlobPart::AnyString])
                                    )))
                                }),
                                Word::Literal(vec![LiteralPart::LineContinuation]),
                                Word::BracedVariable(BracedExpansion {
                                    name: Cow::Borrowed("1"),
                                    modifier: Some(ExpansionModifier::LowerAll(Rc::new(
                                        GlobPattern(vec![GlobPart::AnyString])
                                    )))
                                }),
                                Word::BracedVariable(BracedExpansion {
                                    name: Cow::Borrowed("1"),
                                    modifier: Some(ExpansionModifier::ErrorOnUnset(Rc::new(Text(
                                        vec![TextUnit::Unquoted(vec![Word::Literal(vec![
                                            LiteralPart::String(Cow::Borrowed("err"))
                                        ])])]
                                    ))))
                                }),
                                Word::BracedVariable(BracedExpansion {
                                    name: Cow::Borrowed("1"),
                                    modifier: Some(ExpansionModifier::WhenUnset(Rc::new(Text(
                                        vec![TextUnit::Unquoted(vec![Word::Literal(vec![
                                            LiteralPart::String(Cow::Borrowed("unset"))
                                        ])])]
                                    ))))
                                }),
                                Word::BracedVariable(BracedExpansion {
                                    name: Cow::Borrowed("1"),
                                    modifier: Some(ExpansionModifier::WhenSet(Rc::new(Text(
                                        vec![TextUnit::Unquoted(vec![Word::Literal(vec![
                                            LiteralPart::String(Cow::Borrowed("set"))
                                        ])])]
                                    ))))
                                })
                            ])
                        ])))
                    }),
                    Token::Newline
                ])
            )
        );
        assert_eq!(apml_ast(src).unwrap().1.to_string(), src);
        let src = r##"PKGVER=8.2
PKGDEP="x11-lib libdrm expat systemd elfutils libvdpau nettle \
        libva wayland s2tc lm-sensors libglvnd llvm-runtime libclc"
MESON_AFTER="-Ddri-drivers-path=/usr/lib/xorg/modules/dri \
             -Db_ndebug=true" 
MESON_AFTER__AMD64=" \
             ${MESON_AFTER} \
             -Dlibunwind=true""##;
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
        assert_eq!(token(" ").unwrap(), ("", Token::Spacy(' ')));
        assert_eq!(token("\t").unwrap(), ("", Token::Spacy('\t')));
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
        assert_eq!(text("", " #").unwrap(), ("", Text(vec![])));
        assert_eq!(
            text("asd\\f\\\n134$a'test'\"a$a${a}  \" a", " #").unwrap(),
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
            text("asd\\f\n134$a'test'\"a$a${a}  \" a", " ").unwrap(),
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
            text_unit("asdf134 a", " ").unwrap(),
            (
                " a",
                TextUnit::Unquoted(vec![Word::Literal(vec![LiteralPart::String(
                    Cow::Borrowed("asdf134")
                )])])
            )
        );
        assert_eq!(
            text_unit("'123 a'", " ").unwrap(),
            ("", TextUnit::SingleQuote(Cow::Borrowed("123 a")))
        );
        assert_eq!(
            text_unit("1$a${#b} a$a", " ").unwrap(),
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
            text_unit("\"1\\\na$a${#b}\" a", " ").unwrap(),
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
        text_unit("", " ").unwrap_err();
    }

    #[test]
    fn test_word() {
        assert_eq!(
            word("asdf134 a", " #").unwrap(),
            (
                " a",
                Word::Literal(vec![LiteralPart::String(Cow::Borrowed("asdf134"))])
            )
        );
        assert_eq!(
            word("asdf134 a", "").unwrap(),
            (
                "",
                Word::Literal(vec![LiteralPart::String(Cow::Borrowed("asdf134 a"))])
            )
        );
        assert_eq!(
            word("asdf\\134\\\n a", "").unwrap(),
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
            word("$123 a", "").unwrap(),
            (" a", Word::UnbracedVariable(Cow::Borrowed("123")))
        );
        assert_eq!(
            word("${abc} a", "").unwrap(),
            (
                " a",
                Word::BracedVariable(BracedExpansion {
                    name: Cow::Borrowed("abc"),
                    modifier: None
                })
            )
        );
        assert_eq!(
            word("${#abc} a", "").unwrap(),
            (
                " a",
                Word::BracedVariable(BracedExpansion {
                    name: Cow::Borrowed("abc"),
                    modifier: Some(ExpansionModifier::Length)
                })
            )
        );
        word("${#abc:1} a", "").unwrap_err();
        word("", "").unwrap_err();
        assert_eq!(
            word("${abc:1:2} a", "").unwrap(),
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
        assert_eq!(
            word("${abc#test?} a", "").unwrap(),
            (
                " a",
                Word::BracedVariable(BracedExpansion {
                    name: Cow::Borrowed("abc"),
                    modifier: Some(ExpansionModifier::StripShortestPrefix(Rc::new(
                        GlobPattern(vec![
                            GlobPart::String(Cow::Borrowed("test")),
                            GlobPart::AnyChar
                        ])
                    )))
                })
            )
        );
    }

    #[test]
    fn test_literal_part() {
        assert_eq!(
            literal_part("abc a", " #").unwrap(),
            (" a", LiteralPart::String(Cow::Borrowed("abc")))
        );
        assert_eq!(
            literal_part("abc a", "").unwrap(),
            ("", LiteralPart::String(Cow::Borrowed("abc a")))
        );
        assert_eq!(
            literal_part("\\na a", " ").unwrap(),
            ("a a", LiteralPart::Escaped('n'))
        );
        assert_eq!(
            literal_part("\\\na a", " ").unwrap(),
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
        assert_eq!(
            expansion_modifier("#a*").unwrap(),
            (
                "",
                ExpansionModifier::StripShortestPrefix(Rc::new(GlobPattern(vec![
                    GlobPart::String(Cow::Borrowed("a")),
                    GlobPart::AnyString
                ])))
            )
        );
        assert_eq!(
            expansion_modifier("##a*").unwrap(),
            (
                "",
                ExpansionModifier::StripLongestPrefix(Rc::new(GlobPattern(vec![
                    GlobPart::String(Cow::Borrowed("a")),
                    GlobPart::AnyString
                ])))
            )
        );
        assert_eq!(
            expansion_modifier("%%a*").unwrap(),
            (
                "",
                ExpansionModifier::StripLongestSuffix(Rc::new(GlobPattern(vec![
                    GlobPart::String(Cow::Borrowed("a")),
                    GlobPart::AnyString
                ])))
            )
        );
        assert_eq!(
            expansion_modifier("%a*").unwrap(),
            (
                "",
                ExpansionModifier::StripShortestSuffix(Rc::new(GlobPattern(vec![
                    GlobPart::String(Cow::Borrowed("a")),
                    GlobPart::AnyString
                ])))
            )
        );
        assert_eq!(
            expansion_modifier("/a*/$b}").unwrap(),
            ("}", ExpansionModifier::ReplaceOnce {
                pattern: Rc::new(GlobPattern(vec![
                    GlobPart::String(Cow::Borrowed("a")),
                    GlobPart::AnyString
                ])),
                string: Rc::new(Text(vec![TextUnit::Unquoted(vec![
                    Word::UnbracedVariable(Cow::Borrowed("b"))
                ])]))
            })
        );
        assert_eq!(
            expansion_modifier("/#a*/$b}").unwrap(),
            ("}", ExpansionModifier::ReplacePrefix {
                pattern: Rc::new(GlobPattern(vec![
                    GlobPart::String(Cow::Borrowed("a")),
                    GlobPart::AnyString
                ])),
                string: Rc::new(Text(vec![TextUnit::Unquoted(vec![
                    Word::UnbracedVariable(Cow::Borrowed("b"))
                ])]))
            })
        );
        assert_eq!(
            expansion_modifier("/%a*/$b}").unwrap(),
            ("}", ExpansionModifier::ReplaceSuffix {
                pattern: Rc::new(GlobPattern(vec![
                    GlobPart::String(Cow::Borrowed("a")),
                    GlobPart::AnyString
                ])),
                string: Rc::new(Text(vec![TextUnit::Unquoted(vec![
                    Word::UnbracedVariable(Cow::Borrowed("b"))
                ])]))
            })
        );
        assert_eq!(
            expansion_modifier("^a*}").unwrap(),
            (
                "}",
                ExpansionModifier::UpperOnce(Rc::new(GlobPattern(vec![
                    GlobPart::String(Cow::Borrowed("a")),
                    GlobPart::AnyString
                ])))
            )
        );
        assert_eq!(
            expansion_modifier("^^a*}").unwrap(),
            (
                "}",
                ExpansionModifier::UpperAll(Rc::new(GlobPattern(vec![
                    GlobPart::String(Cow::Borrowed("a")),
                    GlobPart::AnyString
                ])))
            )
        );
        assert_eq!(
            expansion_modifier(",a*}").unwrap(),
            (
                "}",
                ExpansionModifier::LowerOnce(Rc::new(GlobPattern(vec![
                    GlobPart::String(Cow::Borrowed("a")),
                    GlobPart::AnyString
                ])))
            )
        );
        assert_eq!(
            expansion_modifier(",,a*}").unwrap(),
            (
                "}",
                ExpansionModifier::LowerAll(Rc::new(GlobPattern(vec![
                    GlobPart::String(Cow::Borrowed("a")),
                    GlobPart::AnyString
                ])))
            )
        );
        assert_eq!(
            expansion_modifier("^a*}").unwrap(),
            (
                "}",
                ExpansionModifier::UpperOnce(Rc::new(GlobPattern(vec![
                    GlobPart::String(Cow::Borrowed("a")),
                    GlobPart::AnyString
                ])))
            )
        );
        assert_eq!(
            expansion_modifier(":?a$a}").unwrap(),
            (
                "}",
                ExpansionModifier::ErrorOnUnset(Rc::new(Text(vec![TextUnit::Unquoted(vec![
                    Word::Literal(vec![LiteralPart::String(Cow::Borrowed("a"))]),
                    Word::UnbracedVariable(Cow::Borrowed("a")),
                ])])))
            )
        );
        assert_eq!(
            expansion_modifier(":-a${a}}").unwrap(),
            (
                "}",
                ExpansionModifier::WhenUnset(Rc::new(Text(vec![TextUnit::Unquoted(vec![
                    Word::Literal(vec![LiteralPart::String(Cow::Borrowed("a"))]),
                    Word::BracedVariable(BracedExpansion {
                        name: Cow::Borrowed("a"),
                        modifier: None,
                    }),
                ])])))
            )
        );
        assert_eq!(
            expansion_modifier(":+a${#a}}").unwrap(),
            (
                "}",
                ExpansionModifier::WhenSet(Rc::new(Text(vec![TextUnit::Unquoted(vec![
                    Word::Literal(vec![LiteralPart::String(Cow::Borrowed("a"))]),
                    Word::BracedVariable(BracedExpansion {
                        name: Cow::Borrowed("a"),
                        modifier: Some(ExpansionModifier::Length),
                    }),
                ])])))
            )
        );
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
