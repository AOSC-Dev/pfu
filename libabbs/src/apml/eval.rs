//! APML expression evaluator.

use std::{
    cmp::{max, min},
    num::ParseIntError,
};

use regex::{Regex, RegexBuilder};
use thiserror::Error;

use super::{
    ApmlContext,
    tree::{
        ApmlParseTree, ArrayToken, ExpansionModifier, GlobPart, GlobPattern, LiteralPart, Text,
        TextUnit, Token, VariableDefinition, VariableOp, VariableValue, Word,
    },
};

#[derive(Error, Debug)]
pub enum EvalError {
    #[error("Unparsable integer: {0}")]
    UnparsableInt(#[from] ParseIntError),
    #[error("Glob-as-regex error: {0}")]
    RegexError(#[from] regex::Error),
    #[error("Required variable is unset: {0}")]
    Unset(String),
    #[error("Syntax error: {0}")]
    SyntaxError(String),
}

impl From<nom::Err<nom::error::Error<&str>>> for EvalError {
    fn from(value: nom::Err<nom::error::Error<&str>>) -> Self {
        Self::SyntaxError(value.to_string())
    }
}

pub type Result<T> = std::result::Result<T, EvalError>;

pub fn eval_parse_tree(apml: &mut ApmlContext, tree: &ApmlParseTree) -> Result<()> {
    let ApmlParseTree(tokens) = tree;
    for token in tokens {
        eval_token(apml, token)?;
    }
    Ok(())
}

fn eval_token(apml: &mut ApmlContext, token: &Token) -> Result<()> {
    match token {
        Token::Spacy(_) | Token::Newline | Token::Comment(_) => Ok(()),
        Token::Variable(def) => eval_variable_def(apml, def),
    }
}

fn eval_variable_def(apml: &mut ApmlContext, def: &VariableDefinition) -> Result<()> {
    let name = def.name.to_string();
    let value = eval_variable_value(apml, &def.value)?;
    match def.op {
        VariableOp::Assignment => {
            apml.variables.insert(name, value);
            Ok(())
        }
        VariableOp::Append => {
            let value = apml.variables.remove(&name).unwrap_or_default() + value;
            apml.variables.insert(name, value);
            Ok(())
        }
    }
}

fn eval_variable_value(apml: &ApmlContext, value: &VariableValue) -> Result<super::VariableValue> {
    match value {
        VariableValue::String(text) => Ok(super::VariableValue::String(eval_text(apml, text)?)),
        VariableValue::Array(tokens) => {
            let mut result = Vec::new();
            for token in tokens {
                eval_array_token(apml, token, &mut result)?;
            }
            Ok(super::VariableValue::Array(result))
        }
    }
}

fn eval_array_token(
    apml: &ApmlContext,
    token: &ArrayToken,
    values: &mut Vec<String>,
) -> Result<()> {
    match token {
        ArrayToken::Spacy(_) | ArrayToken::Newline | ArrayToken::Comment(_) => Ok(()),
        ArrayToken::Element(text) => {
            let units = &text.0;
            if units.len() == 1 {
                let unit = &units[0];
                match unit {
                    TextUnit::Unquoted(words) | TextUnit::DuobleQuote(words) => {
                        if words.len() == 1 {
                            let word = &words[0];
                            if let Word::BracedVariable(word) = word {
                                if word.modifier == Some(ExpansionModifier::ArrayElements) {
                                    // expand array elements
                                    values.append(
                                        &mut apml
                                            .variables
                                            .get(word.name.as_ref())
                                            .cloned()
                                            .unwrap_or_default()
                                            .into_array(),
                                    );
                                    return Ok(());
                                }
                            }
                        }
                    }
                    TextUnit::SingleQuote(_) => {}
                }
            }
            values.push(eval_text(apml, text)?);
            Ok(())
        }
    }
}

pub fn eval_text(apml: &ApmlContext, text: &Text) -> Result<String> {
    let mut result = String::new();
    let Text(units) = text;
    for unit in units {
        match unit {
            TextUnit::Unquoted(words) | TextUnit::DuobleQuote(words) => {
                for word in words {
                    result.push_str(&eval_word(apml, word)?);
                }
            }
            TextUnit::SingleQuote(text) => result.push_str(&text),
        }
    }
    Ok(result)
}

fn eval_word(apml: &ApmlContext, word: &Word) -> Result<String> {
    match word {
        Word::Literal(parts) => {
            let mut result = String::new();
            for part in parts {
                match part {
                    LiteralPart::String(text) => result.push_str(text),
                    LiteralPart::Escaped(ch) => result.push(*ch),
                    LiteralPart::LineContinuation => {}
                }
            }
            Ok(result)
        }
        Word::UnbracedVariable(var) => Ok(apml
            .variables
            .get(var.as_ref())
            .cloned()
            .unwrap_or_default()
            .into_string()),
        Word::BracedVariable(expansion) => {
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
        Word::Subcommand(_) => Ok(word.to_string()),
    }
}

fn apply_expansion_modifier(
    apml: &ApmlContext,
    modifier: &ExpansionModifier,
    value: super::VariableValue,
) -> Result<String> {
    match modifier {
        ExpansionModifier::Substring { offset, length } => {
            let offset = max(offset.as_ref().trim().parse::<isize>()?, 0) as usize;
            let value = value.into_string();
            if let Some(length) = length {
                let length = length.as_ref().trim().parse::<isize>()?;
                if length > 0 {
                    Ok(value[offset..min(offset + length as usize, value.len())].to_string())
                } else {
                    Ok(value[offset..(value.len() - (-length) as usize)].to_string())
                }
            } else {
                Ok(value[offset..].to_string())
            }
        }
        ExpansionModifier::StripShortestPrefix(pattern) => {
            Ok(glob_to_regex(pattern, "^(?:", ")?(.*)$", false)?
                .replace(&value.into_string(), MatchReplacer(2))
                .to_string())
        }
        ExpansionModifier::StripLongestPrefix(pattern) => {
            Ok(glob_to_regex(pattern, "^(?:", ")?(.*?)$", true)?
                .replace(&value.into_string(), MatchReplacer(2))
                .to_string())
        }
        ExpansionModifier::StripShortestSuffix(pattern) => {
            Ok(glob_to_regex(pattern, "^(.*)(?:", ")$", false)?
                .replace(&value.into_string(), MatchReplacer(1))
                .to_string())
        }
        ExpansionModifier::StripLongestSuffix(pattern) => {
            Ok(glob_to_regex(pattern, "^(.*?)(?:", ")$", true)?
                .replace(&value.into_string(), MatchReplacer(1))
                .to_string())
        }
        ExpansionModifier::ReplaceOnce { pattern, string } => match string {
            None => Ok(glob_to_regex(pattern, "", "", true)?
                .replace(&value.into_string(), "")
                .to_string()),
            Some(text) => Ok(glob_to_regex(pattern, "", "", true)?
                .replace(&value.into_string(), &eval_text(apml, &text)?)
                .to_string()),
        },
        ExpansionModifier::ReplaceAll { pattern, string } => match string {
            None => Ok(glob_to_regex(pattern, "", "", true)?
                .replace_all(&value.into_string(), "")
                .to_string()),
            Some(text) => Ok(glob_to_regex(pattern, "", "", true)?
                .replace_all(&value.into_string(), &eval_text(apml, &text)?)
                .to_string()),
        },
        ExpansionModifier::ReplacePrefix { pattern, string } => match string {
            None => Ok(glob_to_regex(pattern, "^", "", true)?
                .replace_all(&value.into_string(), "")
                .to_string()),
            Some(text) => Ok(glob_to_regex(pattern, "^", "", true)?
                .replace_all(&value.into_string(), &eval_text(apml, &text)?)
                .to_string()),
        },
        ExpansionModifier::ReplaceSuffix { pattern, string } => match string {
            None => Ok(glob_to_regex(pattern, "", "$", true)?
                .replace_all(&value.into_string(), "")
                .to_string()),
            Some(text) => Ok(glob_to_regex(pattern, "", "$", true)?
                .replace_all(&value.into_string(), &eval_text(apml, &text)?)
                .to_string()),
        },
        ExpansionModifier::UpperOnce(pattern) => Ok(glob_to_regex(pattern, "", "", true)?
            .replace(&value.into_string(), UppercaseReplacer)
            .to_string()),
        ExpansionModifier::UpperAll(pattern) => Ok(glob_to_regex(pattern, "", "", true)?
            .replace_all(&value.into_string(), UppercaseReplacer)
            .to_string()),
        ExpansionModifier::LowerOnce(pattern) => Ok(glob_to_regex(pattern, "", "", true)?
            .replace(&value.into_string(), LowercaseReplacer)
            .to_string()),
        ExpansionModifier::LowerAll(pattern) => Ok(glob_to_regex(pattern, "", "", true)?
            .replace_all(&value.into_string(), LowercaseReplacer)
            .to_string()),
        ExpansionModifier::ErrorOnUnset(text) => {
            if value.is_null() {
                Err(EvalError::Unset(eval_text(apml, text)?))
            } else {
                Ok(value.into_string())
            }
        }
        ExpansionModifier::Length => Ok(value.len().to_string()),
        ExpansionModifier::WhenUnset(text) => {
            if value.is_null() {
                eval_text(apml, text)
            } else {
                Ok(value.into_string())
            }
        }
        ExpansionModifier::WhenSet(text) => {
            if !value.is_null() {
                eval_text(apml, text)
            } else {
                Ok(value.into_string())
            }
        }
        ExpansionModifier::ArrayElements => Ok(value.into_string()),
        ExpansionModifier::SingleWordElements => Ok(value.into_string()),
    }
}

fn glob_to_regex(pattern: &GlobPattern, pre: &str, post: &str, greedy: bool) -> Result<Regex> {
    let mut result = String::from(pre);
    let GlobPattern(parts) = pattern;
    for part in parts {
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
    result.push_str(post);
    let result = RegexBuilder::new(&result)
        .case_insensitive(false)
        .multi_line(true)
        .unicode(true)
        .build()?;
    Ok(result)
}

struct MatchReplacer(usize);

impl regex::Replacer for MatchReplacer {
    fn replace_append(&mut self, caps: &regex::Captures<'_>, dst: &mut String) {
        dst.push_str(&caps[self.0]);
    }
}

struct UppercaseReplacer;

impl regex::Replacer for UppercaseReplacer {
    fn replace_append(&mut self, caps: &regex::Captures<'_>, dst: &mut String) {
        dst.push_str(&caps[0].to_ascii_uppercase());
    }
}

struct LowercaseReplacer;

impl regex::Replacer for LowercaseReplacer {
    fn replace_append(&mut self, caps: &regex::Captures<'_>, dst: &mut String) {
        dst.push_str(&caps[0].to_ascii_lowercase());
    }
}
