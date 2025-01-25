//! ACBS Package Metadata Language (APML) syntax tree and parsers.

use std::{
	collections::HashMap,
	fmt::{Display, Write},
	ops::{Add, AddAssign, Index},
};

use ast::{ApmlAst, AstNode};
use lst::ApmlLst;
use thiserror::Error;

pub mod ast;
pub mod editor;
pub mod eval;
pub mod lst;
pub mod parser;
pub mod pattern;
pub mod value;

/// A evaluated APML context.
#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct ApmlContext {
	variables: HashMap<String, VariableValue>,
}

impl ApmlContext {
	/// Creates a empty APML context.
	pub fn new() -> Self {
		Default::default()
	}

	/// Evaluates a APML AST, expanding variables.
	pub fn eval_ast(ast: &ApmlAst) -> std::result::Result<Self, ApmlError> {
		let mut apml = ApmlContext::default();
		eval::eval_ast(&mut apml, ast)?;
		Ok(apml)
	}

	/// Emits and evaluates a APML LST.
	pub fn eval_lst(lst: &ApmlLst) -> std::result::Result<Self, ApmlError> {
		Self::eval_ast(&ApmlAst::emit_from(lst)?)
	}

	/// Parses a APML source code, expanding variables.
	pub fn eval_source(src: &str) -> std::result::Result<Self, ApmlError> {
		Self::eval_lst(&ApmlLst::parse(src)?)
	}

	/// Gets a variable value.
	#[must_use]
	pub fn get(&self, name: &str) -> Option<&VariableValue> {
		self.variables.get(name)
	}

	/// Gets a variable value or returns a default value if not found.
	#[must_use]
	pub fn read(&self, name: &str) -> VariableValue {
		self.variables.get(name).cloned().unwrap_or_default()
	}

	/// Gets a variable value.
	#[must_use]
	pub fn get_mut(&mut self, name: &str) -> Option<&mut VariableValue> {
		self.variables.get_mut(name)
	}

	/// Removes a variable value.
	pub fn remove(&mut self, name: &str) -> Option<VariableValue> {
		self.variables.remove(name)
	}

	/// Inserts a variable.
	pub fn insert(&mut self, name: String, value: VariableValue) {
		self.variables.insert(name, value);
	}

	/// Iterates over all variables.
	pub fn iter(&self) -> impl Iterator<Item = (&String, &VariableValue)> {
		self.variables.iter()
	}

	/// Iterates over all variable names.
	pub fn keys(&self) -> impl Iterator<Item = &String> {
		self.variables.keys()
	}

	/// Returns if a variable is defined.
	pub fn contains_var<S: AsRef<str>>(&self, key: S) -> bool {
		self.variables.contains_key(key.as_ref())
	}
}

impl<S: AsRef<str>> Index<S> for ApmlContext {
	type Output = VariableValue;

	fn index(&self, index: S) -> &Self::Output {
		self.get(index.as_ref())
			.expect("no value found for variable")
	}
}

impl IntoIterator for ApmlContext {
	type Item = (String, VariableValue);

	type IntoIter = <HashMap<String, VariableValue> as IntoIterator>::IntoIter;

	fn into_iter(self) -> Self::IntoIter {
		self.variables.into_iter()
	}
}

#[derive(Debug, Error)]
pub enum ApmlError {
	#[error(transparent)]
	Parse(#[from] parser::ParseError),
	#[error(transparent)]
	Emit(#[from] ast::EmitError),
	#[error(transparent)]
	Eval(#[from] eval::EvalError),
}

/// Value of variables.
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum VariableValue {
	String(String),
	Array(Vec<String>),
}

impl VariableValue {
	/// Returns the value as an string.
	///
	/// If the value is an array, it will be converted into a space-delimited
	/// string.
	#[must_use]
	pub fn as_string(&self) -> String {
		match self {
			VariableValue::String(text) => text.to_owned(),
			VariableValue::Array(els) => els.join(" "),
		}
	}

	/// Returns the value as an string.
	///
	/// If the value is an array, it will be converted into a space-delimited
	/// string.
	#[must_use]
	pub fn into_string(self) -> String {
		match self {
			VariableValue::String(text) => text,
			VariableValue::Array(els) => els.join(" "),
		}
	}

	/// Returns the value as an array.
	///
	/// If the value is a string value, it will be split by spaces.
	/// If the string is empty, it will be converted into a empty array.
	#[must_use]
	pub fn as_array(&self) -> Vec<String> {
		match self {
			VariableValue::String(text) => {
				if text.is_empty() {
					vec![]
				} else {
					self.clone().into_array()
				}
			}
			VariableValue::Array(els) => els.to_owned(),
		}
	}

	/// Returns the value as an array.
	///
	/// If the value is a string value, it will be split by spaces.
	/// If the string is empty, it will be converted into a empty array.
	#[must_use]
	pub fn into_array(self) -> Vec<String> {
		match self {
			VariableValue::String(text) => {
				if text.is_empty() {
					vec![]
				} else {
					let mut entries = Vec::new();
					let mut buffer = String::with_capacity(23);
					for char in text.chars() {
						match char {
							' ' | '\t' | '\n' => {
								if !buffer.is_empty() {
									entries.push(buffer);
									buffer = String::with_capacity(23);
								}
							}
							_ => {
								buffer.push(char);
							}
						}
					}
					if !buffer.is_empty() {
						entries.push(buffer);
					}
					entries
				}
			}
			VariableValue::Array(els) => els,
		}
	}

	/// Returns the length of string or array.
	#[must_use]
	pub fn len(&self) -> usize {
		match self {
			VariableValue::String(text) => text.len(),
			VariableValue::Array(els) => els.len(),
		}
	}

	/// Returns if the value is empty.
	#[must_use]
	pub fn is_empty(&self) -> bool {
		match self {
			VariableValue::String(text) => text.is_empty(),
			VariableValue::Array(els) => els.is_empty(),
		}
	}
}

impl Default for VariableValue {
	fn default() -> Self {
		Self::String(String::new())
	}
}

impl Add for VariableValue {
	type Output = Self;

	fn add(self, rhs: Self) -> Self::Output {
		match self {
			Self::String(val) => {
				Self::String(format!("{}{}", val, rhs.into_string()))
			}
			Self::Array(mut val) => {
				val.append(&mut rhs.into_array());
				Self::Array(val)
			}
		}
	}
}

impl AddAssign for VariableValue {
	fn add_assign(&mut self, rhs: Self) {
		match self {
			Self::String(val) => val.push_str(&rhs.into_string()),
			Self::Array(val) => val.append(&mut rhs.into_array()),
		}
	}
}

impl AddAssign<&Self> for VariableValue {
	fn add_assign(&mut self, rhs: &Self) {
		match self {
			Self::String(val) => val.push_str(&rhs.as_string()),
			Self::Array(val) => val.append(&mut rhs.as_array()),
		}
	}
}

impl<S: AsRef<str>> Add<S> for VariableValue {
	type Output = Self;

	fn add(self, rhs: S) -> Self::Output {
		match self {
			Self::String(val) => {
				Self::String(format!("{}{}", val, rhs.as_ref()))
			}
			Self::Array(mut val) => {
				val.push(rhs.as_ref().to_string());
				Self::Array(val)
			}
		}
	}
}

impl<S: AsRef<str>> AddAssign<S> for VariableValue {
	fn add_assign(&mut self, rhs: S) {
		match self {
			Self::String(val) => val.push_str(rhs.as_ref()),
			Self::Array(val) => val.push(rhs.as_ref().to_string()),
		}
	}
}

impl<S: AsRef<str>> From<S> for VariableValue {
	fn from(value: S) -> Self {
		Self::String(value.as_ref().to_string())
	}
}

impl<S: AsRef<str>> PartialEq<S> for VariableValue {
	fn eq(&self, other: &S) -> bool {
		self.as_string() == other.as_ref()
	}
}

impl Display for VariableValue {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::String(val) => {
				f.write_char('\'')?;
				f.write_str(&val.replace('\'', "'\\''"))?;
				f.write_char('\'')?;
				Ok(())
			}
			Self::Array(val) => {
				f.write_char('(')?;
				for (idx, val) in (1..).zip(val) {
					if idx != 1 {
						f.write_char(' ')?;
					}
					f.write_char('\'')?;
					f.write_str(&val.replace('\'', "'\\''"))?;
					f.write_char('\'')?;
				}
				f.write_char(')')?;
				Ok(())
			}
		}
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_variable_value_string() {
		assert_eq!(VariableValue::default().as_string(), "");
		assert_eq!(VariableValue::String("test".into()).as_string(), "test");
		assert_eq!(VariableValue::String("test".into()).into_string(), "test");
		assert_eq!(VariableValue::String("test".into()).as_array(), vec![
			"test".to_string()
		]);
		assert_eq!(
			VariableValue::String("".into()).as_array(),
			Vec::<String>::new()
		);
		assert_eq!(VariableValue::String("test".into()).into_array(), vec![
			"test".to_string()
		]);
		assert_eq!(
			VariableValue::String("".into()).into_array(),
			Vec::<String>::new()
		);
		assert_eq!(VariableValue::String("test".into()).len(), 4);
		assert!(VariableValue::String("".into()).is_empty());
		assert!(!VariableValue::String("test".into()).is_empty());
		assert_eq!(
			VariableValue::String("test".into())
				+ "test" + VariableValue::String("test".into()),
			"testtesttest"
		);
		assert_eq!(
			format!("{}", VariableValue::String("test'test".into())),
			"'test'\\''test'"
		);

		let long_str =
			"1234567890123456789012345678901234567890123456789012345";
		let array = VariableValue::String(long_str.to_string()).into_array();
		assert_eq!(array.len(), 1);
		assert_eq!(array, vec![long_str.to_string()]);
		let array = VariableValue::String(format!(
			"{long_str} {long_str} 1 {long_str}"
		))
		.into_array();
		assert_eq!(array.len(), 4);
		assert_eq!(array, vec![
			long_str.to_string(),
			long_str.to_string(),
			"1".to_string(),
			long_str.to_string()
		]);
	}

	#[test]
	fn test_variable_value_array() {
		assert!(VariableValue::default().as_array().is_empty());
		assert_eq!(
			VariableValue::Array(vec!["test".into()]).as_string(),
			"test"
		);
		assert_eq!(
			VariableValue::Array(vec!["test".into()]).into_string(),
			"test"
		);
		assert_eq!(VariableValue::Array(vec!["test".into()]).as_array(), vec![
			"test".to_string()
		]);
		assert_eq!(
			VariableValue::Array(vec!["test".into()]).into_array(),
			vec!["test".to_string()]
		);
		assert_eq!(VariableValue::Array(vec!["test".into()]).len(), 1);
		assert!(VariableValue::Array(vec![]).is_empty());
		assert!(!VariableValue::Array(vec!["".into()]).is_empty());
		assert!(!VariableValue::Array(vec!["test".into()]).is_empty());
		assert_eq!(
			VariableValue::Array(vec!["test".into()])
				+ VariableValue::Array(vec!["test".into()])
				+ "test",
			VariableValue::Array(vec![
				"test".into(),
				"test".into(),
				"test".into()
			])
		);
		assert_eq!(
			format!(
				"{}",
				VariableValue::Array(vec![
					"test'test".into(),
					"test".into(),
					"test".into()
				])
			),
			"('test'\\''test' 'test' 'test')"
		);
	}

	#[test]
	fn test_apml_context() {
		let mut apml = ApmlContext::eval_source(
			r##"
VAR1=("test")
VAR1+=("b")
A="${VAR1[@]}"
B="${VAR1[*]}"
"##,
		)
		.unwrap();
		dbg!(&apml);
		let apml2 = apml.clone();
		assert_eq!(apml["VAR1"], "test b");
		assert_eq!(apml["A"], "test b");
		assert_eq!(apml["B"], "test b");
		assert!(apml.contains_var("VAR1"));
		assert!(!apml.contains_var("nonexistence"));
		assert_eq!(apml.get("VAR1").unwrap(), &"test b");
		assert_eq!(apml.read("VAR1"), "test b");
		assert_eq!(apml.read("VAR2"), "");
		assert_eq!(apml.get_mut("VAR1").unwrap(), &"test b");
		*apml.get_mut("VAR1").unwrap() += "c";
		*apml.get_mut("VAR1").unwrap() +=
			std::convert::Into::<VariableValue>::into("c");
		*apml.get_mut("VAR1").unwrap() += apml2.read("B");
		*apml.get_mut("VAR1").unwrap() += apml2.read("VAR1");
		*apml.get_mut("VAR1").unwrap() += apml2.read("nonexistence");
		*apml.get_mut("VAR1").unwrap() += apml2.get("B").unwrap();
		*apml.get_mut("VAR1").unwrap() += apml2.get("VAR1").unwrap();
		assert_eq!(
			apml.get_mut("VAR1").unwrap(),
			&"test b c c test b test b test b test b"
		);
		*apml.get_mut("B").unwrap() += "c";
		*apml.get_mut("B").unwrap() += apml2.get("B").unwrap();
		*apml.get_mut("B").unwrap() += apml2.read("B");
		assert_eq!(apml.get_mut("B").unwrap(), &"test bctest btest b");
		assert_eq!(apml.remove("A"), Some("test b".into()));
		assert_eq!(apml.remove("A"), None);
		assert_eq!(apml.get("A"), None);
		apml.insert("A".to_string(), "test".into());
		assert_eq!(apml["A"], "test");
		{
			let mut keys = apml.keys().collect::<Vec<_>>();
			keys.sort();
			assert_eq!(keys, vec!["A", "B", "VAR1"]);
		}
		{
			let mut entries = apml.iter().map(|(k, _)| k).collect::<Vec<_>>();
			entries.sort();
			assert_eq!(entries, vec!["A", "B", "VAR1"]);
		}
		{
			let mut entries =
				apml.clone().into_iter().map(|(k, _)| k).collect::<Vec<_>>();
			entries.sort();
			assert_eq!(entries, vec!["A", "B", "VAR1"]);
		}
	}
}
