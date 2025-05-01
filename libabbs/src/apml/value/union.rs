//! Unions with tags and properties.

use std::{collections::HashMap, sync::Arc};

use kstring::KString;
use nom::{
	Parser,
	branch::alt,
	bytes::complete::{tag, take, take_while, take_while1},
	character::complete::space0,
	combinator::{not, opt, recognize},
	multi::{many0, separated_list1},
	sequence::{pair, preceded, separated_pair},
};

use crate::apml::{lst, parser::ParseError};

/// A union with a tag and properties.
#[derive(Debug, Clone, Default)]
pub struct Union {
	pub tag: KString,
	pub properties: HashMap<KString, String>,
	pub argument: Option<String>,
}

impl Union {
	/// Creates a union without properties and argument.
	pub fn new<S: AsRef<str>>(tag: S) -> Self {
		Self {
			tag: KString::from_ref(tag.as_ref()),
			..Default::default()
		}
	}

	/// Formats the union into a string.
	pub fn print(&self) -> String {
		let mut value = String::from(self.tag.as_str());
		let mut entries = self.properties.iter().collect::<Vec<_>>();
		if !entries.is_empty() {
			entries.sort_by_key(|(k, _)| k.as_str());
			value.push_str("::");
			for (idx, (k, v)) in entries.into_iter().enumerate() {
				if idx != 0 {
					value.push(';');
				}
				value.push_str(k.as_str());
				value.push('=');
				value.push_str(v);
			}
		}
		if let Some(argument) = &self.argument {
			value.push_str("::");
			value.push_str(argument);
		}
		value
	}

	/// Formats the union into a LST text.
	pub fn print_lst(&self) -> lst::Text<'static> {
		lst::Text(vec![lst::TextUnit::DoubleQuote(vec![lst::Word::Literal(
			lst::LiteralPart::escape(self.print()),
		)])])
	}
}

impl TryFrom<&str> for Union {
	type Error = ParseError;

	fn try_from(value: &str) -> Result<Self, Self::Error> {
		let src = value.trim();
		let (i, (_, tag, properties, argument)) = (
			space0,
			take_while1(|ch: char| ch.is_ascii_alphanumeric()),
			opt(preceded(
				tag("::"),
				separated_list1(
					tag(";"),
					separated_pair(
						preceded(
							take_while(|ch: char| {
								ch.is_ascii_whitespace()
									|| ch == '\n' || ch == '\r'
							}),
							take_while1(|ch: char| {
								ch.is_ascii_alphanumeric()
									|| ch == '-' || ch == '_'
							}),
						),
						tag("="),
						recognize(many0(pair(
							not(alt((tag("::"), tag(";")))),
							take(1usize),
						))),
					),
				),
			)),
			opt(preceded(tag("::"), take_while1(|ch: char| ch.is_ascii()))),
		)
			.parse(src)?;
		if !i.is_empty() {
			return Err(ParseError::UnexpectedSource {
				pos: nom::Offset::offset(src, i) + 1,
			});
		}
		let mut props = HashMap::new();
		if let Some(properties) = properties {
			for (k, v) in properties {
				props.insert(KString::from_ref(k), v.to_string());
			}
		}
		Ok(Self {
			tag: KString::from_ref(tag),
			properties: props,
			argument: argument.map(String::from),
		})
	}
}

impl From<&Union> for lst::VariableValue<'_> {
	fn from(value: &Union) -> Self {
		Self::String(Arc::new(value.print_lst()))
	}
}

#[cfg(test)]
mod test {
	use crate::apml::lst;

	use super::Union;

	#[test]
	fn test_union() {
		let mut union = Union::new("test");
		assert_eq!(union.print(), "test");
		assert_eq!(union.print_lst().to_string(), "\"test\"");
		assert_eq!(lst::VariableValue::from(&union).to_string(), "\"test\"");
		union.argument = Some("test".to_string());
		assert_eq!(union.print(), "test::test");
		assert_eq!(union.print_lst().to_string(), "\"test::test\"");
		union.properties.insert("test".into(), "test".to_string());
		assert_eq!(union.print(), "test::test=test::test");
		assert_eq!(union.print_lst().to_string(), "\"test::test=test::test\"");
		union.properties.insert("test1".into(), "test".to_string());
		assert_eq!(
			union.print_lst().to_string(),
			"\"test::test=test;test1=test::test\""
		);
		assert_eq!(union.print(), "test::test=test;test1=test::test");
	}

	#[test]
	fn test_union_parse() {
		let union = Union::try_from("a::b=c;c=d::https://example.org").unwrap();
		assert_eq!(union.tag, "a");
		assert_eq!(union.properties.get("b").unwrap(), "c");
		assert_eq!(union.properties.get("c").unwrap(), "d");
		assert_eq!(union.argument.unwrap(), "https://example.org");
		let union = Union::try_from(
			"a::     b=https://a.com/b;\n   c=d::https://example.org",
		)
		.unwrap();
		assert_eq!(union.tag, "a");
		assert_eq!(union.properties.get("b").unwrap(), "https://a.com/b");
		assert_eq!(union.properties.get("c").unwrap(), "d");
		assert_eq!(union.argument.unwrap(), "https://example.org");
		let union =
			Union::try_from("a::b=c;copy-repo=d::https://example.org").unwrap();
		assert_eq!(union.properties.get("copy-repo").unwrap(), "d");
		let union =
			Union::try_from("a::b=c;copy-repo=https://example.org").unwrap();
		assert_eq!(
			union.properties.get("copy-repo").unwrap(),
			"https://example.org"
		);
		let union = Union::try_from("a::b=c;c=d").unwrap();
		assert_eq!(union.tag, "a");
		assert_eq!(union.properties.get("b").unwrap(), "c");
		assert_eq!(union.properties.get("c").unwrap(), "d");
		assert_eq!(union.argument, None);
		let union = Union::try_from("a::b=c").unwrap();
		assert_eq!(union.tag, "a");
		assert_eq!(union.properties.get("b").unwrap(), "c");
		assert_eq!(union.properties.get("c"), None);
		assert_eq!(union.argument, None);
		let union = Union::try_from("a::https://example.org").unwrap();
		assert_eq!(union.tag, "a");
		assert_eq!(union.argument.unwrap(), "https://example.org");
		let union = Union::try_from("a::b=c::https://example.org").unwrap();
		assert_eq!(
			union.print_lst().to_string(),
			"\"a::b=c::https://example.org\""
		);
		let union = Union::try_from("   a::url=https://example/example.json;pattern=\"latest-runtime\": \"(6\\..+?)\"").unwrap();
		assert_eq!(union.tag, "a");
		assert_eq!(
			union.properties.get("url").unwrap(),
			"https://example/example.json"
		);
	}
}
