//! Lossless syntax tree representation of APML.
//!
//! This LST is designed to correspond to the source file
//! byte by byte in order to obtain a lossless reverse
//! conversion capability to the source file.
//!
//! To parse a source string into LST, see [`ApmlLst::parse`] and [`parser`][super::parser].
//!
//! The root of LST is [`ApmlLst`], which is made up by a list of [tokens](Token).
//!
//! A token may be a [`VariableDefinition`], a space-like character, or a newline.
//! For example, `TEST=value\n` can be parsed into a [`VariableDefinition`] token and a newline token.
//!
//! <div class="warning">
//! The LST structure poses few of limitations and validations.
//! It is your duty to make sure the generated LST is valid, or else
//! the serialized APML may be invalid.
//! </div>

use std::{
	borrow::Cow,
	fmt::{Debug, Display, Write},
	sync::Arc,
};

use super::{
	parser::{ParseError, apml_lst},
	pattern::BashPattern,
};

/// A APML parse-tree, consisting of a list of tokens.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ApmlLst<'a>(pub Vec<Token<'a>>);

impl Display for ApmlLst<'_> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		for token in &self.0 {
			Display::fmt(token, f)?;
		}
		Ok(())
	}
}

impl<'a> ApmlLst<'a> {
	/// Parses a APML source string into a lossless syntax tree.
	///
	/// This is a wrapper calling [`apml_lst`] parser combinator,
	/// while errors produced by the parser are converted into [`ParseError`],
	/// and [`ParseError::UnexpectedSource`] is produced when
	/// there are some unparsable texts in the input.
	pub fn parse(src: &'a str) -> Result<Self, ParseError> {
		let (out, tree) = apml_lst(src)?;
		if !out.is_empty() {
			return Err(ParseError::UnexpectedSource {
				pos: nom::Offset::offset(src, out) + 1,
			});
		}
		Ok(tree)
	}
}

/// A token in the LST.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Token<'a> {
	/// A space-like character (`'<char>'`).
	///
	/// This currently includes:
	/// - Space (`' '`)
	/// - Tab (`'\t'`)
	Spacy(char),
	/// A newline character (`'\n'`, ASCII code 0x0A).
	Newline,
	/// A comment (`"#<text>"`).
	Comment(Cow<'a, str>),
	/// A variable definition.
	Variable(VariableDefinition<'a>),
}

impl Token<'_> {
	/// Returns if the token is a space or a newline.
	pub fn is_empty(&self) -> bool {
		matches!(&self, Token::Newline | Token::Spacy(_))
	}
}

impl Display for Token<'_> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Token::Spacy(ch) => f.write_char(*ch),
			Token::Newline => f.write_char('\n'),
			Token::Comment(text) => f.write_fmt(format_args!("#{}", text)),
			Token::Variable(def) => Display::fmt(def, f),
		}
	}
}

/// A variable definition (`"<name>=<value>"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VariableDefinition<'a> {
	/// Name of the variable.
	pub name: Cow<'a, str>,
	/// Binary operator.
	pub op: VariableOp,
	/// Value of the variable.
	pub value: VariableValue<'a>,
}

impl Display for VariableDefinition<'_> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(&self.name)?;
		Display::fmt(&self.op, f)?;
		Display::fmt(&self.value, f)?;
		Ok(())
	}
}

/// A variable operator.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VariableOp {
	/// Value assignment (`'='`).
	Assignment,
	/// Appending (`"+="`).
	Append,
}

impl Display for VariableOp {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			VariableOp::Assignment => f.write_char('='),
			VariableOp::Append => f.write_str("+="),
		}
	}
}

/// Value of a variable.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VariableValue<'a> {
	/// A string value (`"<text>"`).
	String(Arc<Text<'a>>),
	/// A array value (`"(<tokens>)"`)
	Array(Vec<ArrayToken<'a>>),
}

impl Display for VariableValue<'_> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			VariableValue::String(text) => Display::fmt(text, f),
			VariableValue::Array(tokens) => {
				f.write_char('(')?;
				for token in tokens {
					Display::fmt(token, f)?;
				}
				f.write_char(')')?;
				Ok(())
			}
		}
	}
}

/// A section of text.
///
/// Text is made up of several text units.
/// For example:
/// - `abc'123'` is made up of an unquoted unit `abc` and a single-quoted unit `123`.
/// - `"abc$0"` is made up of one double-quoted unit.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Text<'a>(pub Vec<TextUnit<'a>>);

impl Display for Text<'_> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		for unit in &self.0 {
			Display::fmt(unit, f)?;
		}
		Ok(())
	}
}

/// A unit of text.
///
/// See [Text] and [Word] for more documentation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TextUnit<'a> {
	/// An unquoted text unit (`"<words>"`).
	Unquoted(Vec<Word<'a>>),
	/// A single-quoted text unit (`"'<text>'"`).
	SingleQuote(Cow<'a, str>),
	/// A double-quoted text unit (`"\"<words>\""`).
	DoubleQuote(Vec<Word<'a>>),
}

impl Display for TextUnit<'_> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			TextUnit::Unquoted(words) => {
				for word in words {
					Display::fmt(word, f)?;
				}
				Ok(())
			}
			TextUnit::SingleQuote(text) => {
				f.write_fmt(format_args!("'{}'", text))
			}
			TextUnit::DoubleQuote(words) => {
				f.write_char('"')?;
				for word in words {
					Display::fmt(word, f)?;
				}
				f.write_char('"')?;
				Ok(())
			}
		}
	}
}

/// A word is a part of a text unit.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Word<'a> {
	/// A literal string (`"<parts>"`).
	Literal(Vec<LiteralPart<'a>>),
	/// An unbraced variable expansion (`"$<var>"`).
	UnbracedVariable(Cow<'a, str>),
	/// A braced variable expansion (`"${<expansion>}"`).
	BracedVariable(BracedExpansion<'a>),
	/// A sub-command expansion (`"$(<tokens>)"`).
	Subcommand(Vec<ArrayToken<'a>>),
}

impl Display for Word<'_> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Word::Literal(parts) => {
				for part in parts {
					Display::fmt(part, f)?;
				}
				Ok(())
			}
			Word::UnbracedVariable(name) => {
				f.write_fmt(format_args!("${}", name))
			}
			Word::BracedVariable(exp) => {
				f.write_fmt(format_args!("${{{}}}", exp))
			}
			Word::Subcommand(tokens) => {
				f.write_str("$(")?;
				for token in tokens {
					Display::fmt(token, f)?;
				}
				f.write_str(")")?;
				Ok(())
			}
		}
	}
}

/// A element of literal words.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LiteralPart<'a> {
	/// A string (`"<text>"`).
	String(Cow<'a, str>),
	/// An escaped character (`"\\<char>"`).
	Escaped(char),
	/// A tag for discard newlines (`"\\\n"`).
	LineContinuation,
}

impl Display for LiteralPart<'_> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			LiteralPart::String(text) => f.write_str(text),
			LiteralPart::Escaped(ch) => f.write_fmt(format_args!("\\{}", ch)),
			LiteralPart::LineContinuation => f.write_str("\\\n"),
		}
	}
}

impl LiteralPart<'_> {
	/// Returns if a character should be escaped when used in double-quoted words.
	pub fn should_escape(ch: char) -> bool {
		matches!(ch, '$' | '"' | '\\')
	}

	/// Produces a list of literal part, escaping characters that need to be
	/// escaped when used in double-quoted words.
	///
	/// Note that all escaped strings will be cloned,
	/// so this function needs more allocations.
	pub fn escape<S: AsRef<str>>(text: S) -> Vec<Self> {
		let mut result = Vec::new();
		let mut buffer = String::new();
		for ch in text.as_ref().chars() {
			if Self::should_escape(ch) {
				if !buffer.is_empty() {
					result.push(LiteralPart::String(Cow::Owned(buffer)));
					buffer = String::new();
				}
				result.push(LiteralPart::Escaped(ch));
			} else {
				buffer.push(ch);
			}
		}
		if !buffer.is_empty() {
			result.push(LiteralPart::String(Cow::Owned(buffer)));
		}
		result
	}
}

/// A braced variable expansion (`"<name>[modifier]"`).
///
/// Note that for [ExpansionModifier::Length], the format is `"#<name>"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BracedExpansion<'a> {
	/// Name of the variable.
	pub name: Cow<'a, str>,
	/// Modifier to apply to the expanded value.
	pub modifier: Option<ExpansionModifier<'a>>,
}

impl Display for BracedExpansion<'_> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match &self.modifier {
			Some(ExpansionModifier::Length) => {
				f.write_fmt(format_args!("#{}", self.name))
			}
			None => f.write_str(&self.name),
			Some(modifier) => {
				f.write_fmt(format_args!("{}{}", self.name, modifier))
			}
		}
	}
}

/// A modifier in the braced variable expansion.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExpansionModifier<'a> {
	/// Reference to a substring (`":offset"` or `":offset:length"`).
	///
	/// The range is [offset, (offset+length)) (indexing from zero).
	Substring {
		/// Offset.
		offset: Cow<'a, str>,
		/// Length.
		length: Option<Cow<'a, str>>,
	},
	/// Stripping the shortest matching prefix (`"#<pattern>"`).
	StripShortestPrefix(Arc<BashPattern<'a>>),
	/// Stripping the longest matching prefix (`"##<pattern>"`).
	StripLongestPrefix(Arc<BashPattern<'a>>),
	/// Stripping the shortest matching suffix (`"%<pattern>"`).
	StripShortestSuffix(Arc<BashPattern<'a>>),
	/// Stripping the longest matching suffix (`"%%<pattern>"`).
	StripLongestSuffix(Arc<BashPattern<'a>>),
	/// Replacing the first match of a pattern with a text (`"/<pattern>[/<string>]"`).
	///
	/// `string` can be omitted, leaving `"/<pattern>"` structure,
	/// which removes the first match of the pattern.
	ReplaceOnce {
		pattern: Arc<BashPattern<'a>>,
		string: Option<Arc<Text<'a>>>,
	},
	/// Replacing the all matches of a pattern with a text (`"//<pattern>[/<string>]"`).
	///
	/// `string` can be omitted.
	ReplaceAll {
		pattern: Arc<BashPattern<'a>>,
		string: Option<Arc<Text<'a>>>,
	},
	/// Replacing the prefix of a pattern with a text (`"/#<pattern>[/<string>]"`).
	///
	/// `string` can be omitted.
	ReplacePrefix {
		pattern: Arc<BashPattern<'a>>,
		string: Option<Arc<Text<'a>>>,
	},
	/// Replacing the suffix of a pattern with a text (`"/%<pattern>[/<string>]"`).
	///
	/// `string` can be omitted.
	ReplaceSuffix {
		pattern: Arc<BashPattern<'a>>,
		string: Option<Arc<Text<'a>>>,
	},
	/// Upper-casify the first match of a pattern (`"^<pattern>"`).
	UpperOnce(Arc<BashPattern<'a>>),
	/// Upper-casify the all matches of a pattern (`"^^<pattern>"`).
	UpperAll(Arc<BashPattern<'a>>),
	/// Lower-casify the first match of a pattern (`",<pattern>"`).
	LowerOnce(Arc<BashPattern<'a>>),
	/// Lower-casify the all matches of a pattern (`",,<pattern>"`).
	LowerAll(Arc<BashPattern<'a>>),
	/// Producing errors when the variable is unset or null (`":?<text>"`).
	ErrorOnUnset(Arc<Text<'a>>),
	/// Returning the length of the variable.
	///
	/// Note that this modifier uses a special format, see [BracedExpansion].
	Length,
	/// Returning a text when the variable is unset or null (`":-<text>"`).
	WhenUnset(Arc<Text<'a>>),
	/// Returning a text when the variable is set (`":+<text>"`).
	WhenSet(Arc<Text<'a>>),
	/// Expands to array elements (`"[@]"`).
	ArrayElements,
	/// Expands to a string of array elements concatenated with space (`"[*]"`).
	SingleWordElements,
}

impl Display for ExpansionModifier<'_> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			ExpansionModifier::Substring { offset, length } => match length {
				None => f.write_fmt(format_args!(":{}", offset)),
				Some(length) => {
					f.write_fmt(format_args!(":{}:{}", offset, length))
				}
			},
			ExpansionModifier::StripShortestPrefix(pattern) => {
				f.write_fmt(format_args!("#{}", pattern))
			}
			ExpansionModifier::StripLongestPrefix(pattern) => {
				f.write_fmt(format_args!("##{}", pattern))
			}
			ExpansionModifier::StripShortestSuffix(pattern) => {
				f.write_fmt(format_args!("%{}", pattern))
			}
			ExpansionModifier::StripLongestSuffix(pattern) => {
				f.write_fmt(format_args!("%%{}", pattern))
			}
			ExpansionModifier::ReplaceOnce { pattern, string } => {
				match string {
					Some(string) => {
						f.write_fmt(format_args!("/{}/{}", pattern, string))
					}
					None => f.write_fmt(format_args!("/{}", pattern)),
				}
			}
			ExpansionModifier::ReplaceAll { pattern, string } => match string {
				Some(string) => {
					f.write_fmt(format_args!("//{}/{}", pattern, string))
				}
				None => f.write_fmt(format_args!("//{}", pattern)),
			},
			ExpansionModifier::ReplacePrefix { pattern, string } => {
				match string {
					Some(string) => {
						f.write_fmt(format_args!("/#{}/{}", pattern, string))
					}
					None => f.write_fmt(format_args!("/#{}", pattern)),
				}
			}
			ExpansionModifier::ReplaceSuffix { pattern, string } => {
				match string {
					Some(string) => {
						f.write_fmt(format_args!("/%{}/{}", pattern, string))
					}
					None => f.write_fmt(format_args!("/%{}", pattern)),
				}
			}
			ExpansionModifier::UpperOnce(pattern) => {
				f.write_fmt(format_args!("^{}", pattern))
			}
			ExpansionModifier::UpperAll(pattern) => {
				f.write_fmt(format_args!("^^{}", pattern))
			}
			ExpansionModifier::LowerOnce(pattern) => {
				f.write_fmt(format_args!(",{}", pattern))
			}
			ExpansionModifier::LowerAll(pattern) => {
				f.write_fmt(format_args!(",,{}", pattern))
			}
			ExpansionModifier::ErrorOnUnset(text) => {
				f.write_fmt(format_args!(":?{}", text))
			}
			ExpansionModifier::Length => f.write_char('#'),
			ExpansionModifier::WhenUnset(text) => {
				f.write_fmt(format_args!(":-{}", text))
			}
			ExpansionModifier::WhenSet(text) => {
				f.write_fmt(format_args!(":+{}", text))
			}
			ExpansionModifier::ArrayElements => f.write_str("[@]"),
			ExpansionModifier::SingleWordElements => f.write_str("[*]"),
		}
	}
}

/// A token in an array variable value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ArrayToken<'a> {
	/// A space-like character (`'<char>'`).
	///
	/// See [Token::Spacy] for more.
	Spacy(char),
	/// A newline character (`'\n'`, ASCII code 0x0A).
	Newline,
	/// A comment (`"#<text>"`).
	Comment(Cow<'a, str>),
	/// A array element (`"<text>"`).
	Element(Arc<Text<'a>>),
}

impl Display for ArrayToken<'_> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			ArrayToken::Spacy(ch) => f.write_char(*ch),
			ArrayToken::Newline => f.write_char('\n'),
			ArrayToken::Comment(text) => {
				f.write_char('#')?;
				f.write_str(text)?;
				Ok(())
			}
			ArrayToken::Element(text) => Display::fmt(text, f),
		}
	}
}
#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_apml_parse() {
		let tree = ApmlLst::parse(r##"# Test APML"##).unwrap();
		dbg!(&tree);
		let tree = ApmlLst::parse(r##"aaa"##).unwrap_err();
		dbg!(&tree);
	}

	#[test]
	fn test_token() {
		assert!(Token::Newline.is_empty());
		assert!(Token::Spacy(' ').is_empty());
		assert!(Token::Spacy('\t').is_empty());
		assert!(!Token::Comment(Cow::Borrowed("Test")).is_empty());
		assert!(
			!Token::Variable(VariableDefinition {
				name: Cow::Borrowed("Test"),
				op: VariableOp::Assignment,
				value: VariableValue::String(Arc::new(Text(vec![])))
			})
			.is_empty()
		);
	}

	#[test]
	fn test_literal_part_escape() {
		assert!(LiteralPart::should_escape('$'));
		assert!(LiteralPart::should_escape('"'));
		assert!(LiteralPart::should_escape('\\'));
		assert!(!LiteralPart::should_escape('a'));
		assert!(!LiteralPart::should_escape(' '));
		assert_eq!(
			LiteralPart::escape("asdf"),
			vec![LiteralPart::String("asdf".into())]
		);
		assert_eq!(
			LiteralPart::escape("asd$$f\ng\\"),
			vec![
				LiteralPart::String("asd".into()),
				LiteralPart::Escaped('$'),
				LiteralPart::Escaped('$'),
				LiteralPart::String("f\ng".into()),
				LiteralPart::Escaped('\\'),
			]
		);
	}
}
