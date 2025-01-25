//! APML expression evaluator.

use std::cmp::min;

use thiserror::Error;

use super::{ApmlContext, VariableValue, ast};

#[derive(Error, Debug)]
pub enum EvalError {
	#[error("Glob-as-regex error: {0}")]
	RegexError(#[from] regex::Error),
	#[error("Required variable is unset: {0}")]
	Unset(String),
}

type Result<T> = std::result::Result<T, EvalError>;

pub fn eval_ast(apml: &mut ApmlContext, tree: &ast::ApmlAst) -> Result<()> {
	let ast::ApmlAst(defs) = tree;
	for def in defs {
		eval_variable_def(apml, def)?;
	}
	Ok(())
}

#[inline]
fn eval_variable_def(
	apml: &mut ApmlContext,
	def: &ast::VariableDefinition,
) -> Result<()> {
	let name = def.name.to_string();
	let value = eval_variable_value(apml, &def.value)?;
	apml.variables.insert(name, value);
	Ok(())
}

#[inline]
fn eval_variable_value(
	apml: &ApmlContext,
	value: &ast::VariableValue,
) -> Result<VariableValue> {
	match value {
		ast::VariableValue::String(text) => {
			Ok(VariableValue::String(eval_text(apml, text)?))
		}
		ast::VariableValue::Array(element) => {
			let mut result = Vec::new();
			for element in element {
				eval_array_element(apml, element, &mut result)?;
			}
			Ok(VariableValue::Array(result))
		}
	}
}

#[inline]
fn eval_array_element(
	apml: &ApmlContext,
	element: &ast::ArrayElement,
	values: &mut Vec<String>,
) -> Result<()> {
	match element {
		ast::ArrayElement::ArrayInclusion(name) => {
			// expand array elements
			values.append(
				&mut apml
					.variables
					.get(name.as_ref())
					.cloned()
					.unwrap_or_default()
					.into_array(),
			);
			Ok(())
		}
		ast::ArrayElement::Text(text) => {
			values.push(eval_text(apml, text)?);
			Ok(())
		}
	}
}

pub fn eval_text(apml: &ApmlContext, text: &ast::Text) -> Result<String> {
	let mut result = String::new();
	let ast::Text(words) = text;
	for word in words {
		result.push_str(&eval_word(apml, word)?);
	}
	Ok(result)
}

#[inline]
fn eval_word(apml: &ApmlContext, word: &ast::Word) -> Result<String> {
	match word {
		ast::Word::Literal(text) | ast::Word::Subcommand(text) => {
			Ok(text.to_string())
		}
		ast::Word::Variable(expansion) => {
			let val = apml
				.variables
				.get(expansion.name.as_ref())
				.cloned()
				.unwrap_or_default();
			if let Some(modifier) = &expansion.modifier {
				apply_expansion_modifier(apml, modifier, val)
			} else {
				Ok(val.into_string())
			}
		}
	}
}

fn apply_expansion_modifier(
	apml: &ApmlContext,
	modifier: &ast::ExpansionModifier,
	value: VariableValue,
) -> Result<String> {
	struct MatchReplacer(usize);
	impl regex::Replacer for MatchReplacer {
		fn replace_append(
			&mut self,
			caps: &regex::Captures<'_>,
			dst: &mut String,
		) {
			dst.push_str(&caps[self.0]);
		}
	}

	struct UppercaseReplacer;
	impl regex::Replacer for UppercaseReplacer {
		fn replace_append(
			&mut self,
			caps: &regex::Captures<'_>,
			dst: &mut String,
		) {
			dst.push_str(&caps[0].to_ascii_uppercase());
		}
	}

	struct LowercaseReplacer;
	impl regex::Replacer for LowercaseReplacer {
		fn replace_append(
			&mut self,
			caps: &regex::Captures<'_>,
			dst: &mut String,
		) {
			dst.push_str(&caps[0].to_ascii_lowercase());
		}
	}

	match modifier {
		ast::ExpansionModifier::Substring { offset, length } => {
			let value = value.into_string();
			if let Some(length) = length {
				if *length > 0 {
					Ok(value
						[*offset..min(*offset + *length as usize, value.len())]
						.to_string())
				} else {
					Ok(value[*offset..(value.len() - (-*length) as usize)]
						.to_string())
				}
			} else {
				Ok(value[*offset..].to_string())
			}
		}
		ast::ExpansionModifier::StripShortestPrefix(pattern) => Ok(pattern
			.to_regex("^(?:", ")?(.*)$", false)?
			.replace(&value.into_string(), MatchReplacer(1))
			.to_string()),
		ast::ExpansionModifier::StripLongestPrefix(pattern) => Ok(pattern
			.to_regex("^(?:", ")?(.*?)$", true)?
			.replace(&value.into_string(), MatchReplacer(1))
			.to_string()),
		ast::ExpansionModifier::StripShortestSuffix(pattern) => Ok(pattern
			.to_regex("^(.*)(?:", ")$", false)?
			.replace(&value.into_string(), MatchReplacer(1))
			.to_string()),
		ast::ExpansionModifier::StripLongestSuffix(pattern) => Ok(pattern
			.to_regex("^(.*?)(?:", ")$", true)?
			.replace(&value.into_string(), MatchReplacer(1))
			.to_string()),
		ast::ExpansionModifier::ReplaceOnce { pattern, string } => Ok(pattern
			.to_regex("", "", true)?
			.replace(&value.into_string(), &eval_text(apml, string)?)
			.to_string()),
		ast::ExpansionModifier::ReplaceAll { pattern, string } => Ok(pattern
			.to_regex("", "", true)?
			.replace_all(&value.into_string(), &eval_text(apml, string)?)
			.to_string()),
		ast::ExpansionModifier::ReplacePrefix { pattern, string } => {
			Ok(pattern
				.to_regex("^", "", true)?
				.replace_all(&value.into_string(), &eval_text(apml, string)?)
				.to_string())
		}
		ast::ExpansionModifier::ReplaceSuffix { pattern, string } => {
			Ok(pattern
				.to_regex("", "$", true)?
				.replace_all(&value.into_string(), &eval_text(apml, string)?)
				.to_string())
		}
		ast::ExpansionModifier::UpperOnce(pattern) => Ok(pattern
			.to_regex("", "", true)?
			.replace(&value.into_string(), UppercaseReplacer)
			.to_string()),
		ast::ExpansionModifier::UpperAll(pattern) => Ok(pattern
			.to_regex("", "", true)?
			.replace_all(&value.into_string(), UppercaseReplacer)
			.to_string()),
		ast::ExpansionModifier::LowerOnce(pattern) => Ok(pattern
			.to_regex("", "", true)?
			.replace(&value.into_string(), LowercaseReplacer)
			.to_string()),
		ast::ExpansionModifier::LowerAll(pattern) => Ok(pattern
			.to_regex("", "", true)?
			.replace_all(&value.into_string(), LowercaseReplacer)
			.to_string()),
		ast::ExpansionModifier::ErrorOnUnset(text) => {
			if value.is_empty() {
				Err(EvalError::Unset(eval_text(apml, text)?))
			} else {
				Ok(value.into_string())
			}
		}
		ast::ExpansionModifier::Length => Ok(value.len().to_string()),
		ast::ExpansionModifier::WhenUnset(text) => {
			if value.is_empty() {
				eval_text(apml, text)
			} else {
				Ok(value.into_string())
			}
		}
		ast::ExpansionModifier::WhenSet(text) => {
			if !value.is_empty() {
				eval_text(apml, text)
			} else {
				Ok(value.into_string())
			}
		}
	}
}

#[cfg(test)]
mod test {
	use std::sync::Arc;

	use crate::apml::{
		ApmlContext,
		ast::{ExpansionModifier, Text, Word},
		eval::apply_expansion_modifier,
		pattern::{BashPattern, GlobPart},
	};

	#[test]
	fn test_expansion_modifier() {
		let mut ctx = ApmlContext::new();
		ctx.insert("A".to_string(), "test".into());

		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::Substring {
					offset: 0,
					length: Some(10)
				},
				"123".into()
			)
			.unwrap(),
			"123"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::Substring {
					offset: 0,
					length: Some(-1)
				},
				"123".into()
			)
			.unwrap(),
			"12"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::Substring {
					offset: 1,
					length: None
				},
				"123".into()
			)
			.unwrap(),
			"23"
		);
		let pattern1 = Arc::new(BashPattern(vec![
			GlobPart::String("a".into()),
			GlobPart::AnyString,
		]));
		let pattern2 = Arc::new(BashPattern(vec![
			GlobPart::String("a".into()),
			GlobPart::AnyChar,
		]));
		let text1 = Arc::new(Text(vec![Word::Literal("test".into())]));
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::StripShortestPrefix(pattern1.clone()),
				"123".into()
			)
			.unwrap(),
			"123"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::StripShortestPrefix(pattern1.clone()),
				"a123".into()
			)
			.unwrap(),
			"123"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::StripShortestPrefix(pattern1.clone()),
				"123".into()
			)
			.unwrap(),
			"123"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::StripShortestSuffix(pattern1.clone()),
				"a123a123".into()
			)
			.unwrap(),
			"a123"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::StripLongestPrefix(pattern1.clone()),
				"123".into()
			)
			.unwrap(),
			"123"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::StripLongestPrefix(pattern1.clone()),
				"a123".into()
			)
			.unwrap(),
			""
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::StripLongestSuffix(pattern1.clone()),
				"123".into()
			)
			.unwrap(),
			"123"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::StripLongestSuffix(pattern1.clone()),
				"a123a123".into()
			)
			.unwrap(),
			""
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::ReplaceOnce {
					pattern: pattern1.clone(),
					string: text1.clone()
				},
				"1a123a123".into()
			)
			.unwrap(),
			"1test"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::ReplaceOnce {
					pattern: pattern2.clone(),
					string: text1.clone()
				},
				"a123a123".into()
			)
			.unwrap(),
			"test23a123"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::ReplaceAll {
					pattern: pattern1.clone(),
					string: text1.clone()
				},
				"1a123a123".into()
			)
			.unwrap(),
			"1test"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::ReplaceAll {
					pattern: pattern2.clone(),
					string: text1.clone()
				},
				"a123a123".into()
			)
			.unwrap(),
			"test23test23"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::ReplacePrefix {
					pattern: pattern1.clone(),
					string: text1.clone()
				},
				"1a123a123".into()
			)
			.unwrap(),
			"1a123a123"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::ReplacePrefix {
					pattern: pattern1.clone(),
					string: text1.clone()
				},
				"a123a123".into()
			)
			.unwrap(),
			"test"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::ReplaceSuffix {
					pattern: pattern1.clone(),
					string: text1.clone()
				},
				"1a123a1231".into()
			)
			.unwrap(),
			"1test"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::ReplaceSuffix {
					pattern: pattern1.clone(),
					string: text1.clone()
				},
				"a123a123".into()
			)
			.unwrap(),
			"test"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::UpperOnce(pattern1.clone()),
				"aa123abc123".into()
			)
			.unwrap(),
			"AA123ABC123"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::UpperOnce(pattern2.clone()),
				"aa123abc123".into()
			)
			.unwrap(),
			"AA123abc123"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::UpperAll(pattern1.clone()),
				"aa123abc123".into()
			)
			.unwrap(),
			"AA123ABC123"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::UpperAll(pattern2.clone()),
				"aa123abc123".into()
			)
			.unwrap(),
			"AA123ABc123"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::LowerOnce(pattern1.clone()),
				"aA123aBC123".into()
			)
			.unwrap(),
			"aa123abc123"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::LowerOnce(pattern2.clone()),
				"aA123aBC123".into()
			)
			.unwrap(),
			"aa123aBC123"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::LowerAll(pattern1.clone()),
				"aA123aBC123".into()
			)
			.unwrap(),
			"aa123abc123"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::LowerAll(pattern2.clone()),
				"aA123aBc123".into()
			)
			.unwrap(),
			"aa123abc123"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::ErrorOnUnset(text1.clone()),
				"test".into()
			)
			.unwrap(),
			"test"
		);
		apply_expansion_modifier(
			&ctx,
			&ExpansionModifier::ErrorOnUnset(text1.clone()),
			"".into(),
		)
		.unwrap_err();
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::Length,
				"test".into()
			)
			.unwrap(),
			"4"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::WhenUnset(text1.clone()),
				"aaa".into()
			)
			.unwrap(),
			"aaa"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::WhenUnset(text1.clone()),
				"".into()
			)
			.unwrap(),
			"test"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::WhenSet(text1.clone()),
				"aaa".into()
			)
			.unwrap(),
			"test"
		);
		assert_eq!(
			apply_expansion_modifier(
				&ctx,
				&ExpansionModifier::WhenSet(text1.clone()),
				"".into()
			)
			.unwrap(),
			""
		);
	}
}
