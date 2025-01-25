//! Parser combinators to parse APML source code to [LST][super::lst].

use std::{borrow::Cow, sync::Arc};

use nom::{
	IResult,
	branch::alt,
	bytes::complete::{tag, take, take_till, take_while, take_while1},
	character::complete::{anychar, char, newline, one_of},
	combinator::{map, opt, recognize, value},
	multi::{many0, many1},
	sequence::{delimited, pair, preceded, tuple},
};
use thiserror::Error;

use crate::apml::pattern::{BashPattern, bash_pattern};

use super::lst::*;

/// Errors produced while parsing the input source.
#[derive(Debug, Error)]
pub enum ParseError {
	#[error("Syntax error: {0}")]
	SyntaxError(String),
	#[error("Unexpected source at char {pos}")]
	UnexpectedSource { pos: usize },
}

impl From<nom::Err<nom::error::Error<&str>>> for ParseError {
	fn from(value: nom::Err<nom::error::Error<&str>>) -> Self {
		Self::SyntaxError(value.to_string())
	}
}

/// Parses a complete APML source into LST.
pub fn apml_lst(i: &str) -> IResult<&str, ApmlLst> {
	map(many0(token), ApmlLst)(i)
}

#[inline]
fn token(i: &str) -> IResult<&str, Token> {
	alt((
		// spacy
		map(spacy_char, Token::Spacy),
		// newline
		value(Token::Newline, newline),
		// comment
		comment_token,
		// variable definition
		map(variable_def, Token::Variable),
	))(i)
}

#[inline]
fn spacy_char(i: &str) -> IResult<&str, char> {
	alt((char(' '), char('\t')))(i)
}

#[inline]
fn comment_token(i: &str) -> IResult<&str, Token> {
	map(preceded(char('#'), take_till(|ch| ch == '\n')), |comment| {
		Token::Comment(Cow::Borrowed(comment))
	})(i)
}

#[inline]
fn variable_def(i: &str) -> IResult<&str, VariableDefinition> {
	map(
		tuple((variable_name, variable_op, variable_value)),
		|(name, op, value)| VariableDefinition {
			name: Cow::Borrowed(name),
			op,
			value,
		},
	)(i)
}

#[inline]
fn variable_op(i: &str) -> IResult<&str, VariableOp> {
	alt((
		value(VariableOp::Assignment, char('=')),
		value(VariableOp::Append, tag("+=")),
	))(i)
}

#[inline]
fn variable_name(i: &str) -> IResult<&str, &str> {
	take_while1(|ch: char| ch.is_alphanumeric() || ch == '_')(i)
}

#[inline]
fn variable_value(i: &str) -> IResult<&str, VariableValue> {
	alt((
		// array
		map(
			delimited(char('('), many0(array_token), char(')')),
			VariableValue::Array,
		),
		// string
		map(
			|s| text_or_null(s, &|ch| ch != ' ' && ch != '#'),
			|text| VariableValue::String(Arc::new(text)),
		),
	))(i)
}

#[inline]
fn array_token(i: &str) -> IResult<&str, ArrayToken> {
	alt((
		// spacy
		map(spacy_char, ArrayToken::Spacy),
		// newline
		value(ArrayToken::Newline, newline),
		//comment
		map(preceded(char('#'), take_till(|ch| ch == '\n')), |comment| {
			ArrayToken::Comment(Cow::Borrowed(comment))
		}),
		// element
		map(
			|s| text(s, &|ch| ch != ' ' && ch != '#' && ch != ')'),
			|text| ArrayToken::Element(Arc::new(text)),
		),
	))(i)
}

#[inline]
fn text<'a, Cond>(i: &'a str, cond: &Cond) -> IResult<&'a str, Text<'a>>
where
	Cond: Fn(char) -> bool,
{
	map(many1(|s| text_unit(s, &cond)), Text)(i)
}

#[inline]
fn text_or_null<'a, Cond>(i: &'a str, cond: &Cond) -> IResult<&'a str, Text<'a>>
where
	Cond: Fn(char) -> bool,
{
	map(many0(|s| text_unit(s, &cond)), Text)(i)
}

#[inline]
fn text_unit<'a, Cond>(
	i: &'a str,
	cond: &Cond,
) -> IResult<&'a str, TextUnit<'a>>
where
	Cond: Fn(char) -> bool,
{
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
			map(
				many0(|s| word(s, &|_| true, &one_of("$\\\"`"))),
				TextUnit::DoubleQuote,
			),
			char('"'),
		),
		// unquoted
		map(
			many1(|s| {
				word(s, &|ch| cond(ch) && ch != '\'' && ch != '\n', &anychar)
			}),
			TextUnit::Unquoted,
		),
	))(i)
}

#[inline]
fn word<'a, Cond, EscCond>(
	i: &'a str,
	cond: &Cond,
	escape_cond: &EscCond,
) -> IResult<&'a str, Word<'a>>
where
	Cond: Fn(char) -> bool,
	EscCond: Fn(&'a str) -> IResult<&'a str, char>,
{
	alt((
		// braced variable
		map(delimited(tag("${"), braced_expansion, char('}')), |exp| {
			Word::BracedVariable(exp)
		}),
		// unbraced variable
		map(preceded(char('$'), variable_name), |name| {
			Word::UnbracedVariable(Cow::Borrowed(name))
		}),
		// subcommand
		map(
			delimited(tag("$("), many0(array_token), char(')')),
			Word::Subcommand,
		),
		// literal
		map(many1(|s| literal_part(s, cond, escape_cond)), Word::Literal),
	))(i)
}

#[inline]
fn literal_part<'a, Cond, EscCond>(
	i: &'a str,
	literal_cond: &Cond,
	escape_cond: &EscCond,
) -> IResult<&'a str, LiteralPart<'a>>
where
	Cond: Fn(char) -> bool,
	EscCond: Fn(&'a str) -> IResult<&'a str, char>,
{
	alt((
		// line continuation
		value(LiteralPart::LineContinuation, tag("\\\n")),
		// escaped
		map(preceded(char('\\'), escape_cond), LiteralPart::Escaped),
		// invalid escaped
		map(recognize(pair(char('\\'), take(1usize))), |s| {
			LiteralPart::String(Cow::Borrowed(s))
		}),
		// literal
		map(
			take_while1(|ch| !"$\"\\".contains(ch) && literal_cond(ch)),
			|s| LiteralPart::String(Cow::Borrowed(s)),
		),
	))(i)
}

#[inline]
fn braced_expansion(i: &str) -> IResult<&str, BracedExpansion> {
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
fn expansion_modifier(i: &str) -> IResult<&str, ExpansionModifier> {
	#[inline]
	fn expansion_glob(i: &str) -> IResult<&str, Arc<BashPattern>> {
		map(|s| bash_pattern(s, "}"), Arc::new)(i)
	}
	#[inline]
	fn expansion_glob_replace(i: &str) -> IResult<&str, Arc<BashPattern>> {
		map(|s| bash_pattern(s, "}/"), Arc::new)(i)
	}
	#[inline]
	fn expansion_text(i: &str) -> IResult<&str, Arc<Text>> {
		map(|s| text_or_null(s, &|ch| ch != '}'), Arc::new)(i)
	}
	alt((
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
				pair(
					expansion_glob_replace,
					opt(preceded(char('/'), expansion_text)),
				),
			),
			|(pattern, string)| ExpansionModifier::ReplaceAll {
				pattern,
				string,
			},
		),
		map(
			preceded(
				tag("/#"),
				pair(
					expansion_glob_replace,
					opt(preceded(char('/'), expansion_text)),
				),
			),
			|(pattern, string)| ExpansionModifier::ReplacePrefix {
				pattern,
				string,
			},
		),
		map(
			preceded(
				tag("/%"),
				pair(
					expansion_glob_replace,
					opt(preceded(char('/'), expansion_text)),
				),
			),
			|(pattern, string)| ExpansionModifier::ReplaceSuffix {
				pattern,
				string,
			},
		),
		map(
			preceded(
				char('/'),
				pair(
					expansion_glob_replace,
					opt(preceded(char('/'), expansion_text)),
				),
			),
			|(pattern, string)| ExpansionModifier::ReplaceOnce {
				pattern,
				string,
			},
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
		substring_expansion_modifier,
		value(ExpansionModifier::ArrayElements, tag("[@]")),
		value(ExpansionModifier::SingleWordElements, tag("[*]")),
	))(i)
}

#[inline]
fn substring_expansion_modifier(i: &str) -> IResult<&str, ExpansionModifier> {
	#[inline]
	fn number(i: &str) -> IResult<&str, Cow<'_, str>> {
		map(
			take_while1(|ch: char| {
				ch.is_ascii_digit() || " \n-\t".contains(ch)
			}),
			Cow::Borrowed,
		)(i)
	}
	preceded(
		char(':'),
		map(
			pair(number, opt(preceded(char(':'), number))),
			|(offset, length)| ExpansionModifier::Substring { offset, length },
		),
	)(i)
}

#[cfg(test)]
mod test {
	use crate::apml::{
		parser::*,
		pattern::{BashPattern, GlobPart},
	};

	#[test]
	fn test_ast() {
		let src = r##"# Test APML

a=b'c' # Inline comment
K=a"${#a} $ab b\ \\#l \
安安本来是只兔子，但有一天早上醒来发现自己变成了长着兔耳和兔尾巴的人类
c ${1:1}${1:1: -1}${1##a}${1#a.*[:alpha:]b?\?}${1%%1}${1%1}\
${1/a/a}${1//a?a/$a}${1/#a/b}${1/%a/b}${1^*}${1^^*}${1,*}\
${1,,*}${1:?err}${1:-unset}${1:+set}${1/a}${1//a}${1/#a}\
${1/%a}${1//a/}"
b+=("-a" \
    -b "${a[@]}" "${a[*]}$(a)" #asdf
)
"##;
		assert_eq!(
			apml_lst(src).unwrap(),
			(
				"",
				ApmlLst(vec![
					Token::Comment(Cow::Borrowed(" Test APML")),
					Token::Newline,
					Token::Newline,
					Token::Variable(VariableDefinition {
						name: Cow::Borrowed("a"),
						op: VariableOp::Assignment,
						value: VariableValue::String(Arc::new(Text(vec![
							TextUnit::Unquoted(vec![Word::Literal(vec![
								LiteralPart::String(Cow::Borrowed("b"))
							])]),
							TextUnit::SingleQuote(Cow::Borrowed("c"))
						])))
					}),
					Token::Spacy(' '),
					Token::Comment(Cow::Borrowed(" Inline comment")),
					Token::Newline,
					Token::Variable(VariableDefinition {
						name: Cow::Borrowed("K"),
						op: VariableOp::Assignment,
						value: VariableValue::String(Arc::new(Text(vec![
							TextUnit::Unquoted(vec![Word::Literal(vec![
								LiteralPart::String(Cow::Borrowed("a"))
							])]),
							TextUnit::DoubleQuote(vec![
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("a"),
									modifier: Some(ExpansionModifier::Length)
								}),
								Word::Literal(vec![LiteralPart::String(
									Cow::Borrowed(" ")
								),]),
								Word::UnbracedVariable(Cow::Borrowed("ab")),
								Word::Literal(vec![
									LiteralPart::String(Cow::Borrowed(" b")),
									LiteralPart::String(Cow::Borrowed("\\ ")),
									LiteralPart::Escaped('\\'),
									LiteralPart::String(Cow::Borrowed("#l ")),
									LiteralPart::LineContinuation,
									LiteralPart::String(Cow::Borrowed(
										"安安本来是只兔子，但有一天早上醒来发现自己变成了长着兔耳和兔尾巴的人类\nc "
									)),
								]),
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("1"),
									modifier: Some(
										ExpansionModifier::Substring {
											offset: Cow::Borrowed("1"),
											length: None
										}
									)
								}),
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("1"),
									modifier: Some(
										ExpansionModifier::Substring {
											offset: Cow::Borrowed("1"),
											length: Some(Cow::Borrowed(" -1"))
										}
									)
								}),
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("1"),
									modifier: Some(
										ExpansionModifier::StripLongestPrefix(
											Arc::new(BashPattern(vec![
												GlobPart::String(
													Cow::Borrowed("a")
												)
											]))
										)
									)
								}),
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("1"),
									modifier: Some(
										ExpansionModifier::StripShortestPrefix(
											Arc::new(BashPattern(vec![
												GlobPart::String(
													Cow::Borrowed("a.")
												),
												GlobPart::AnyString,
												GlobPart::Range(Cow::Borrowed(
													":alpha:"
												)),
												GlobPart::String(
													Cow::Borrowed("b")
												),
												GlobPart::AnyChar,
												GlobPart::Escaped('?'),
											]))
										)
									)
								}),
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("1"),
									modifier: Some(
										ExpansionModifier::StripLongestSuffix(
											Arc::new(BashPattern(vec![
												GlobPart::String(
													Cow::Borrowed("1")
												)
											]))
										)
									)
								}),
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("1"),
									modifier: Some(
										ExpansionModifier::StripShortestSuffix(
											Arc::new(BashPattern(vec![
												GlobPart::String(
													Cow::Borrowed("1")
												)
											]))
										)
									)
								}),
								Word::Literal(vec![
									LiteralPart::LineContinuation
								]),
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("1"),
									modifier: Some(
										ExpansionModifier::ReplaceOnce {
											pattern: Arc::new(BashPattern(
												vec![GlobPart::String(
													Cow::Borrowed("a")
												)]
											)),
											string: Some(Arc::new(Text(vec![
												TextUnit::Unquoted(vec![
													Word::Literal(vec![
														LiteralPart::String(
															Cow::Borrowed("a")
														)
													])
												])
											])))
										}
									)
								}),
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("1"),
									modifier: Some(
										ExpansionModifier::ReplaceAll {
											pattern: Arc::new(BashPattern(
												vec![
													GlobPart::String(
														Cow::Borrowed("a")
													),
													GlobPart::AnyChar,
													GlobPart::String(
														Cow::Borrowed("a")
													)
												]
											)),
											string: Some(Arc::new(Text(vec![
												TextUnit::Unquoted(vec![
													Word::UnbracedVariable(
														Cow::Borrowed("a")
													)
												])
											])))
										}
									)
								}),
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("1"),
									modifier: Some(
										ExpansionModifier::ReplacePrefix {
											pattern: Arc::new(BashPattern(
												vec![GlobPart::String(
													Cow::Borrowed("a")
												)]
											)),
											string: Some(Arc::new(Text(vec![
												TextUnit::Unquoted(vec![
													Word::Literal(vec![
														LiteralPart::String(
															Cow::Borrowed("b")
														)
													])
												])
											])))
										}
									)
								}),
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("1"),
									modifier: Some(
										ExpansionModifier::ReplaceSuffix {
											pattern: Arc::new(BashPattern(
												vec![GlobPart::String(
													Cow::Borrowed("a")
												)]
											)),
											string: Some(Arc::new(Text(vec![
												TextUnit::Unquoted(vec![
													Word::Literal(vec![
														LiteralPart::String(
															Cow::Borrowed("b")
														)
													])
												])
											])))
										}
									)
								}),
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("1"),
									modifier: Some(
										ExpansionModifier::UpperOnce(Arc::new(
											BashPattern(vec![
												GlobPart::AnyString
											])
										))
									)
								}),
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("1"),
									modifier: Some(
										ExpansionModifier::UpperAll(Arc::new(
											BashPattern(vec![
												GlobPart::AnyString
											])
										))
									)
								}),
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("1"),
									modifier: Some(
										ExpansionModifier::LowerOnce(Arc::new(
											BashPattern(vec![
												GlobPart::AnyString
											])
										))
									)
								}),
								Word::Literal(vec![
									LiteralPart::LineContinuation
								]),
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("1"),
									modifier: Some(
										ExpansionModifier::LowerAll(Arc::new(
											BashPattern(vec![
												GlobPart::AnyString
											])
										))
									)
								}),
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("1"),
									modifier: Some(
										ExpansionModifier::ErrorOnUnset(
											Arc::new(Text(vec![
												TextUnit::Unquoted(vec![
													Word::Literal(vec![
														LiteralPart::String(
															Cow::Borrowed(
																"err"
															)
														)
													])
												])
											]))
										)
									)
								}),
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("1"),
									modifier: Some(
										ExpansionModifier::WhenUnset(Arc::new(
											Text(vec![TextUnit::Unquoted(
												vec![Word::Literal(vec![
													LiteralPart::String(
														Cow::Borrowed("unset")
													)
												])]
											)])
										))
									)
								}),
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("1"),
									modifier: Some(ExpansionModifier::WhenSet(
										Arc::new(Text(vec![
											TextUnit::Unquoted(vec![
												Word::Literal(vec![
													LiteralPart::String(
														Cow::Borrowed("set")
													)
												])
											])
										]))
									))
								}),
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("1"),
									modifier: Some(
										ExpansionModifier::ReplaceOnce {
											pattern: Arc::new(BashPattern(
												vec![GlobPart::String(
													Cow::Borrowed("a")
												)]
											)),
											string: None
										}
									)
								}),
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("1"),
									modifier: Some(
										ExpansionModifier::ReplaceAll {
											pattern: Arc::new(BashPattern(
												vec![GlobPart::String(
													Cow::Borrowed("a")
												)]
											)),
											string: None
										}
									)
								}),
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("1"),
									modifier: Some(
										ExpansionModifier::ReplacePrefix {
											pattern: Arc::new(BashPattern(
												vec![GlobPart::String(
													Cow::Borrowed("a")
												)]
											)),
											string: None
										}
									)
								}),
								Word::Literal(vec![
									LiteralPart::LineContinuation
								]),
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("1"),
									modifier: Some(
										ExpansionModifier::ReplaceSuffix {
											pattern: Arc::new(BashPattern(
												vec![GlobPart::String(
													Cow::Borrowed("a")
												)]
											)),
											string: None
										}
									)
								}),
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("1"),
									modifier: Some(
										ExpansionModifier::ReplaceAll {
											pattern: Arc::new(BashPattern(
												vec![GlobPart::String(
													Cow::Borrowed("a")
												)]
											)),
											string: Some(Arc::new(Text(
												vec![]
											)))
										}
									)
								}),
							])
						])))
					}),
					Token::Newline,
					Token::Variable(VariableDefinition {
						name: Cow::Borrowed("b"),
						op: VariableOp::Append,
						value: VariableValue::Array(vec![
							ArrayToken::Element(Arc::new(Text(vec![
								TextUnit::DoubleQuote(vec![Word::Literal(
									vec![LiteralPart::String(Cow::Borrowed(
										"-a"
									))]
								)])
							]))),
							ArrayToken::Spacy(' '),
							ArrayToken::Element(Arc::new(Text(vec![
								TextUnit::Unquoted(vec![Word::Literal(vec![
									LiteralPart::LineContinuation
								])])
							]))),
							ArrayToken::Spacy(' '),
							ArrayToken::Spacy(' '),
							ArrayToken::Spacy(' '),
							ArrayToken::Spacy(' '),
							ArrayToken::Element(Arc::new(Text(vec![
								TextUnit::Unquoted(vec![Word::Literal(vec![
									LiteralPart::String(Cow::Borrowed("-b"))
								])])
							]))),
							ArrayToken::Spacy(' '),
							ArrayToken::Element(Arc::new(Text(vec![
								TextUnit::DoubleQuote(vec![
									Word::BracedVariable(BracedExpansion {
										name: Cow::Borrowed("a"),
										modifier: Some(
											ExpansionModifier::ArrayElements
										)
									})
								])
							]))),
							ArrayToken::Spacy(' '),
							ArrayToken::Element(Arc::new(Text(vec![
								TextUnit::DoubleQuote(vec![
								Word::BracedVariable(BracedExpansion {
									name: Cow::Borrowed("a"),
									modifier: Some(ExpansionModifier::SingleWordElements)
								}),
								Word::Subcommand(vec![ArrayToken::Element(Arc::new(Text(vec![
									TextUnit::Unquoted(vec![Word::Literal(vec![
										LiteralPart::String(Cow::Borrowed("a"))
									])])
								])))]),
							])
							]))),
							ArrayToken::Spacy(' '),
							ArrayToken::Comment(Cow::Borrowed("asdf")),
							ArrayToken::Newline,
						])
					}),
					Token::Newline,
				])
			)
		);
		assert_eq!(apml_lst(src).unwrap().1.to_string(), src);
		let src = r##"PKGVER=8.2
PKGDEP="x11-lib libdrm expat systemd elfutils libvdpau nettle \
        libva wayland s2tc lm-sensors libglvnd llvm-runtime libclc"
MESON_AFTER="-Ddri-drivers-path=/usr/lib/xorg/modules/dri \
             -Db_ndebug=true" 
MESON_AFTER__AMD64=" \
             ${MESON_AFTER} \
             -Dlibunwind=true""##;
		assert_eq!(apml_lst(src).unwrap().1.to_string(), src);
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
					op: VariableOp::Assignment,
					value: VariableValue::String(Arc::new(Text(vec![])))
				})
			)
		);
	}

	#[test]
	fn test_spacy_char() {
		assert_eq!(spacy_char(" ").unwrap(), ("", ' '));
		assert_eq!(spacy_char("\t").unwrap(), ("", '\t'));
		assert_eq!(spacy_char("\ta").unwrap(), ("a", '\t'));
		spacy_char("\n").unwrap_err();
	}

	#[test]
	fn test_variable_def() {
		variable_def("=\n").unwrap_err();
		variable_def("?=\n").unwrap_err();
		assert_eq!(
			variable_def("a=\n").unwrap(),
			("\n", VariableDefinition {
				name: Cow::Borrowed("a"),
				op: VariableOp::Assignment,
				value: VariableValue::String(Arc::new(Text(vec![])))
			})
		);
		assert_eq!(
			variable_def("a=b$0\n").unwrap(),
			("\n", VariableDefinition {
				name: Cow::Borrowed("a"),
				op: VariableOp::Assignment,
				value: VariableValue::String(Arc::new(Text(vec![
					TextUnit::Unquoted(vec![
						Word::Literal(vec![LiteralPart::String(
							Cow::Borrowed("b")
						)]),
						Word::UnbracedVariable(Cow::Borrowed("0")),
					])
				])))
			})
		);
		assert_eq!(
			variable_def("a+=b$0\n").unwrap(),
			("\n", VariableDefinition {
				name: Cow::Borrowed("a"),
				op: VariableOp::Append,
				value: VariableValue::String(Arc::new(Text(vec![
					TextUnit::Unquoted(vec![
						Word::Literal(vec![LiteralPart::String(
							Cow::Borrowed("b")
						)]),
						Word::UnbracedVariable(Cow::Borrowed("0")),
					])
				])))
			})
		);
	}

	#[test]
	fn test_variable_op() {
		assert_eq!(
			variable_op("=123a").unwrap(),
			("123a", VariableOp::Assignment)
		);
		assert_eq!(
			variable_op("+=123a").unwrap(),
			("123a", VariableOp::Append)
		);
		variable_name("!!!").unwrap_err();
		variable_name("").unwrap_err();
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
			("\n", VariableValue::String(Arc::new(Text(vec![]))))
		);
		assert_eq!(
			variable_value("123\\n\\\na!!@$1#").unwrap(),
			(
				"#",
				VariableValue::String(Arc::new(Text(vec![
					TextUnit::Unquoted(vec![
						Word::Literal(vec![
							LiteralPart::String(Cow::Borrowed("123")),
							LiteralPart::Escaped('n'),
							LiteralPart::LineContinuation,
							LiteralPart::String(Cow::Borrowed("a!!@")),
						]),
						Word::UnbracedVariable(Cow::Borrowed("1")),
					])
				])))
			)
		);
		assert_eq!(
			variable_value("\"${#a} b\\ #l \\\nc\"\n").unwrap(),
			(
				"\n",
				VariableValue::String(Arc::new(Text(vec![
					TextUnit::DoubleQuote(vec![
						Word::BracedVariable(BracedExpansion {
							name: Cow::Borrowed("a"),
							modifier: Some(ExpansionModifier::Length)
						}),
						Word::Literal(vec![
							LiteralPart::String(Cow::Borrowed(" b")),
							LiteralPart::String(Cow::Borrowed("\\ ")),
							LiteralPart::String(Cow::Borrowed("#l ")),
							LiteralPart::LineContinuation,
							LiteralPart::String(Cow::Borrowed("c"))
						])
					])
				])))
			)
		);
		assert_eq!(
			variable_value("(a b)\n").unwrap(),
			(
				"\n",
				VariableValue::Array(vec![
					ArrayToken::Element(Arc::new(Text(vec![
						TextUnit::Unquoted(vec![Word::Literal(vec![
							LiteralPart::String(Cow::Borrowed("a")),
						])])
					]))),
					ArrayToken::Spacy(' '),
					ArrayToken::Element(Arc::new(Text(vec![
						TextUnit::Unquoted(vec![Word::Literal(vec![
							LiteralPart::String(Cow::Borrowed("b")),
						])])
					]))),
				])
			)
		);
		assert_eq!(
			variable_value("(a \"${#a} b\\ \\\\#l \\\nc\"\n)\n").unwrap(),
			(
				"\n",
				VariableValue::Array(vec![
					ArrayToken::Element(Arc::new(Text(vec![
						TextUnit::Unquoted(vec![Word::Literal(vec![
							LiteralPart::String(Cow::Borrowed("a")),
						])])
					]))),
					ArrayToken::Spacy(' '),
					ArrayToken::Element(Arc::new(Text(vec![
						TextUnit::DoubleQuote(vec![
							Word::BracedVariable(BracedExpansion {
								name: Cow::Borrowed("a"),
								modifier: Some(ExpansionModifier::Length)
							}),
							Word::Literal(vec![
								LiteralPart::String(Cow::Borrowed(" b")),
								LiteralPart::String(Cow::Borrowed("\\ ")),
								LiteralPart::Escaped('\\'),
								LiteralPart::String(Cow::Borrowed("#l ")),
								LiteralPart::LineContinuation,
								LiteralPart::String(Cow::Borrowed("c"))
							])
						])
					]))),
					ArrayToken::Newline,
				])
			)
		);
	}

	#[test]
	fn test_array_token() {
		assert_eq!(array_token(" a").unwrap(), ("a", ArrayToken::Spacy(' ')));
		assert_eq!(array_token("\ta").unwrap(), ("a", ArrayToken::Spacy('\t')));
		assert_eq!(array_token("\na").unwrap(), ("a", ArrayToken::Newline));
		assert_eq!(
			array_token("#asdf\na").unwrap(),
			("\na", ArrayToken::Comment(Cow::Borrowed("asdf")))
		);
		assert_eq!(
			array_token("asdf ").unwrap(),
			(
				" ",
				ArrayToken::Element(Arc::new(Text(vec![TextUnit::Unquoted(
					vec![Word::Literal(vec![LiteralPart::String(
						Cow::Borrowed("asdf")
					)])]
				)])))
			)
		);
		assert_eq!(
			array_token("'asdf' ").unwrap(),
			(
				" ",
				ArrayToken::Element(Arc::new(Text(vec![
					TextUnit::SingleQuote(Cow::Borrowed("asdf"))
				])))
			)
		);
	}

	#[test]
	fn test_text() {
		text("", &|_| true).unwrap_err();
		assert_eq!(text_or_null("", &|_| true).unwrap(), ("", Text(vec![])));
		assert_eq!(
			text("asd\\f\\\n134$a'test'\"a$a${a}  \" a", &|ch| ch != ' '
				&& ch != '#')
			.unwrap(),
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
					TextUnit::DoubleQuote(vec![
						Word::Literal(vec![LiteralPart::String(
							Cow::Borrowed("a")
						)]),
						Word::UnbracedVariable(Cow::Borrowed("a")),
						Word::BracedVariable(BracedExpansion {
							name: Cow::Borrowed("a"),
							modifier: None
						}),
						Word::Literal(vec![LiteralPart::String(
							Cow::Borrowed("  ")
						)]),
					])
				])
			)
		);
		assert_eq!(
			text("asd\\f\n134$a'test'\"a$a${a}  \" a", &|ch| ch != ' ')
				.unwrap(),
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
			text_unit("asdf134 a", &|ch| ch != ' ').unwrap(),
			(
				" a",
				TextUnit::Unquoted(vec![Word::Literal(vec![
					LiteralPart::String(Cow::Borrowed("asdf134"))
				])])
			)
		);
		assert_eq!(
			text_unit("'123 a'", &|ch| ch != ' ').unwrap(),
			("", TextUnit::SingleQuote(Cow::Borrowed("123 a")))
		);
		assert_eq!(
			text_unit("1$a${#b}' a$a", &|ch| ch != ' ').unwrap(),
			(
				"' a$a",
				TextUnit::Unquoted(vec![
					Word::Literal(vec![LiteralPart::String(Cow::Borrowed(
						"1"
					))]),
					Word::UnbracedVariable(Cow::Borrowed("a")),
					Word::BracedVariable(BracedExpansion {
						name: Cow::Borrowed("b"),
						modifier: Some(ExpansionModifier::Length),
					}),
				])
			)
		);
		assert_eq!(
			text_unit("\"1\\\na$a${#b}安同'\" a", &|ch| ch != ' ').unwrap(),
			(
				" a",
				TextUnit::DoubleQuote(vec![
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
					Word::Literal(vec![LiteralPart::String(Cow::Borrowed(
						"安同'"
					))]),
				])
			)
		);
		text_unit("", &|ch| ch != ' ').unwrap_err();
	}

	#[test]
	fn test_word() {
		assert_eq!(
			word("asdf134 a", &|ch| ch != ' ', &anychar).unwrap(),
			(
				" a",
				Word::Literal(vec![LiteralPart::String(Cow::Borrowed(
					"asdf134"
				))])
			)
		);
		assert_eq!(
			word("asdf134 a", &|_| true, &anychar).unwrap(),
			(
				"",
				Word::Literal(vec![LiteralPart::String(Cow::Borrowed(
					"asdf134 a"
				))])
			)
		);
		assert_eq!(
			word("asdf\\134\\\n a", &|_| true, &anychar).unwrap(),
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
			word("asdf\\1\\34\\\n a", &|_| true, &one_of("3")).unwrap(),
			(
				"",
				Word::Literal(vec![
					LiteralPart::String(Cow::Borrowed("asdf")),
					LiteralPart::String(Cow::Borrowed("\\1")),
					LiteralPart::Escaped('3'),
					LiteralPart::String(Cow::Borrowed("4")),
					LiteralPart::LineContinuation,
					LiteralPart::String(Cow::Borrowed(" a")),
				])
			)
		);
		assert_eq!(
			word("$123 a", &|ch| ch != ' ', &anychar).unwrap(),
			(" a", Word::UnbracedVariable(Cow::Borrowed("123")))
		);
		assert_eq!(
			word("${abc} a", &|ch| ch != ' ', &anychar).unwrap(),
			(
				" a",
				Word::BracedVariable(BracedExpansion {
					name: Cow::Borrowed("abc"),
					modifier: None
				})
			)
		);
		assert_eq!(
			word("${#abc} a", &|ch| ch != ' ', &anychar).unwrap(),
			(
				" a",
				Word::BracedVariable(BracedExpansion {
					name: Cow::Borrowed("abc"),
					modifier: Some(ExpansionModifier::Length)
				})
			)
		);
		word("${#abc:1} a", &|ch| ch != ' ', &anychar).unwrap_err();
		word("", &|ch| ch != ' ', &anychar).unwrap_err();
		assert_eq!(
			word("${abc:1:2} a", &|ch| ch != ' ', &anychar).unwrap(),
			(
				" a",
				Word::BracedVariable(BracedExpansion {
					name: Cow::Borrowed("abc"),
					modifier: Some(ExpansionModifier::Substring {
						offset: Cow::Borrowed("1"),
						length: Some(Cow::Borrowed("2"))
					})
				})
			)
		);
		assert_eq!(
			word("${abc#test?} a", &|ch| ch != ' ', &anychar).unwrap(),
			(
				" a",
				Word::BracedVariable(BracedExpansion {
					name: Cow::Borrowed("abc"),
					modifier: Some(ExpansionModifier::StripShortestPrefix(
						Arc::new(BashPattern(vec![
							GlobPart::String(Cow::Borrowed("test")),
							GlobPart::AnyChar
						]))
					))
				})
			)
		);
		assert_eq!(
			word("$(123 ) a", &|ch| ch != ' ', &anychar).unwrap(),
			(
				" a",
				Word::Subcommand(vec![
					ArrayToken::Element(Arc::new(Text(vec![
						TextUnit::Unquoted(vec![Word::Literal(vec![
							LiteralPart::String(Cow::Borrowed("123"))
						])])
					]))),
					ArrayToken::Spacy(' ')
				])
			)
		);
	}

	#[test]
	fn test_literal_part() {
		assert_eq!(
			literal_part("abc a", &|ch| ch != ' ', &anychar).unwrap(),
			(" a", LiteralPart::String(Cow::Borrowed("abc")))
		);
		assert_eq!(
			literal_part("abc a", &|_| true, &anychar).unwrap(),
			("", LiteralPart::String(Cow::Borrowed("abc a")))
		);
		assert_eq!(
			literal_part("\\na a", &|ch| ch != ' ', &anychar).unwrap(),
			("a a", LiteralPart::Escaped('n'))
		);
		assert_eq!(
			literal_part("\\na a", &|ch| ch != ' ', &one_of("1")).unwrap(),
			("a a", LiteralPart::String(Cow::Borrowed("\\n")))
		);
		assert_eq!(
			literal_part("\\\na a", &|ch| ch != ' ', &anychar).unwrap(),
			("a a", LiteralPart::LineContinuation)
		);
		assert_eq!(
			literal_part("安安本来是只兔子\n a", &|ch| ch != ' ', &anychar)
				.unwrap(),
			(
				" a",
				LiteralPart::String(Cow::Borrowed("安安本来是只兔子\n"))
			)
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
					offset: Cow::Borrowed("10"),
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
				offset: Cow::Borrowed("10"),
				length: None
			})
		);
		assert_eq!(
			expansion_modifier(":10:1").unwrap(),
			("", ExpansionModifier::Substring {
				offset: Cow::Borrowed("10"),
				length: Some(Cow::Borrowed("1"))
			})
		);
		assert_eq!(
			expansion_modifier(": -10:-1").unwrap(),
			("", ExpansionModifier::Substring {
				offset: Cow::Borrowed(" -10"),
				length: Some(Cow::Borrowed("-1"))
			})
		);
		expansion_modifier(":").unwrap_err();
		expansion_modifier("1").unwrap_err();
		assert_eq!(
			expansion_modifier("#a*").unwrap(),
			(
				"",
				ExpansionModifier::StripShortestPrefix(Arc::new(BashPattern(
					vec![
						GlobPart::String(Cow::Borrowed("a")),
						GlobPart::AnyString
					]
				)))
			)
		);
		assert_eq!(
			expansion_modifier("##a*").unwrap(),
			(
				"",
				ExpansionModifier::StripLongestPrefix(Arc::new(BashPattern(
					vec![
						GlobPart::String(Cow::Borrowed("a")),
						GlobPart::AnyString
					]
				)))
			)
		);
		assert_eq!(
			expansion_modifier("%%a*").unwrap(),
			(
				"",
				ExpansionModifier::StripLongestSuffix(Arc::new(BashPattern(
					vec![
						GlobPart::String(Cow::Borrowed("a")),
						GlobPart::AnyString
					]
				)))
			)
		);
		assert_eq!(
			expansion_modifier("%a*").unwrap(),
			(
				"",
				ExpansionModifier::StripShortestSuffix(Arc::new(BashPattern(
					vec![
						GlobPart::String(Cow::Borrowed("a")),
						GlobPart::AnyString
					]
				)))
			)
		);
		assert_eq!(
			expansion_modifier("/a*/$b}").unwrap(),
			("}", ExpansionModifier::ReplaceOnce {
				pattern: Arc::new(BashPattern(vec![
					GlobPart::String(Cow::Borrowed("a")),
					GlobPart::AnyString
				])),
				string: Some(Arc::new(Text(vec![TextUnit::Unquoted(vec![
					Word::UnbracedVariable(Cow::Borrowed("b"))
				])])))
			})
		);
		assert_eq!(
			expansion_modifier("/a*}").unwrap(),
			("}", ExpansionModifier::ReplaceOnce {
				pattern: Arc::new(BashPattern(vec![
					GlobPart::String(Cow::Borrowed("a")),
					GlobPart::AnyString
				])),
				string: None
			})
		);
		assert_eq!(
			expansion_modifier("//a*/$b}").unwrap(),
			("}", ExpansionModifier::ReplaceAll {
				pattern: Arc::new(BashPattern(vec![
					GlobPart::String(Cow::Borrowed("a")),
					GlobPart::AnyString
				])),
				string: Some(Arc::new(Text(vec![TextUnit::Unquoted(vec![
					Word::UnbracedVariable(Cow::Borrowed("b"))
				])])))
			})
		);
		assert_eq!(
			expansion_modifier("//a*}").unwrap(),
			("}", ExpansionModifier::ReplaceAll {
				pattern: Arc::new(BashPattern(vec![
					GlobPart::String(Cow::Borrowed("a")),
					GlobPart::AnyString
				])),
				string: None
			})
		);
		assert_eq!(
			expansion_modifier("/#a*/$b}").unwrap(),
			("}", ExpansionModifier::ReplacePrefix {
				pattern: Arc::new(BashPattern(vec![
					GlobPart::String(Cow::Borrowed("a")),
					GlobPart::AnyString
				])),
				string: Some(Arc::new(Text(vec![TextUnit::Unquoted(vec![
					Word::UnbracedVariable(Cow::Borrowed("b"))
				])])))
			})
		);
		assert_eq!(
			expansion_modifier("/#a*}").unwrap(),
			("}", ExpansionModifier::ReplacePrefix {
				pattern: Arc::new(BashPattern(vec![
					GlobPart::String(Cow::Borrowed("a")),
					GlobPart::AnyString
				])),
				string: None
			})
		);
		assert_eq!(
			expansion_modifier("/%a*/$b}").unwrap(),
			("}", ExpansionModifier::ReplaceSuffix {
				pattern: Arc::new(BashPattern(vec![
					GlobPart::String(Cow::Borrowed("a")),
					GlobPart::AnyString
				])),
				string: Some(Arc::new(Text(vec![TextUnit::Unquoted(vec![
					Word::UnbracedVariable(Cow::Borrowed("b"))
				])])))
			})
		);
		assert_eq!(
			expansion_modifier("/%a*}").unwrap(),
			("}", ExpansionModifier::ReplaceSuffix {
				pattern: Arc::new(BashPattern(vec![
					GlobPart::String(Cow::Borrowed("a")),
					GlobPart::AnyString
				])),
				string: None
			})
		);
		assert_eq!(
			expansion_modifier("^a*}").unwrap(),
			(
				"}",
				ExpansionModifier::UpperOnce(Arc::new(BashPattern(vec![
					GlobPart::String(Cow::Borrowed("a")),
					GlobPart::AnyString
				])))
			)
		);
		assert_eq!(
			expansion_modifier("^^a*}").unwrap(),
			(
				"}",
				ExpansionModifier::UpperAll(Arc::new(BashPattern(vec![
					GlobPart::String(Cow::Borrowed("a")),
					GlobPart::AnyString
				])))
			)
		);
		assert_eq!(
			expansion_modifier(",a*}").unwrap(),
			(
				"}",
				ExpansionModifier::LowerOnce(Arc::new(BashPattern(vec![
					GlobPart::String(Cow::Borrowed("a")),
					GlobPart::AnyString
				])))
			)
		);
		assert_eq!(
			expansion_modifier(",,a*}").unwrap(),
			(
				"}",
				ExpansionModifier::LowerAll(Arc::new(BashPattern(vec![
					GlobPart::String(Cow::Borrowed("a")),
					GlobPart::AnyString
				])))
			)
		);
		assert_eq!(
			expansion_modifier("^a*}").unwrap(),
			(
				"}",
				ExpansionModifier::UpperOnce(Arc::new(BashPattern(vec![
					GlobPart::String(Cow::Borrowed("a")),
					GlobPart::AnyString
				])))
			)
		);
		assert_eq!(
			expansion_modifier(":?a$a}").unwrap(),
			(
				"}",
				ExpansionModifier::ErrorOnUnset(Arc::new(Text(vec![
					TextUnit::Unquoted(vec![
						Word::Literal(vec![LiteralPart::String(
							Cow::Borrowed("a")
						)]),
						Word::UnbracedVariable(Cow::Borrowed("a")),
					])
				])))
			)
		);
		assert_eq!(
			expansion_modifier(":-a${a}}").unwrap(),
			(
				"}",
				ExpansionModifier::WhenUnset(Arc::new(Text(vec![
					TextUnit::Unquoted(vec![
						Word::Literal(vec![LiteralPart::String(
							Cow::Borrowed("a")
						)]),
						Word::BracedVariable(BracedExpansion {
							name: Cow::Borrowed("a"),
							modifier: None,
						}),
					])
				])))
			)
		);
		assert_eq!(
			expansion_modifier(":+a${#a}}").unwrap(),
			(
				"}",
				ExpansionModifier::WhenSet(Arc::new(Text(vec![
					TextUnit::Unquoted(vec![
						Word::Literal(vec![LiteralPart::String(
							Cow::Borrowed("a")
						)]),
						Word::BracedVariable(BracedExpansion {
							name: Cow::Borrowed("a"),
							modifier: Some(ExpansionModifier::Length),
						}),
					])
				])))
			)
		);
		assert_eq!(
			expansion_modifier("[@]}").unwrap(),
			("}", ExpansionModifier::ArrayElements)
		);
		assert_eq!(
			expansion_modifier("[*]}").unwrap(),
			("}", ExpansionModifier::SingleWordElements)
		);
	}

	#[test]
	fn test_substring_expansion_modifier() {
		assert_eq!(
			substring_expansion_modifier(":10").unwrap(),
			("", ExpansionModifier::Substring {
				offset: Cow::Borrowed("10"),
				length: None
			})
		);
		assert_eq!(
			substring_expansion_modifier(":10:1").unwrap(),
			("", ExpansionModifier::Substring {
				offset: Cow::Borrowed("10"),
				length: Some(Cow::Borrowed("1"))
			})
		);
		assert_eq!(
			substring_expansion_modifier(": -10:1").unwrap(),
			("", ExpansionModifier::Substring {
				offset: Cow::Borrowed(" -10"),
				length: Some(Cow::Borrowed("1"))
			})
		);
		substring_expansion_modifier(":").unwrap_err();
		substring_expansion_modifier("1").unwrap_err();
	}
}
