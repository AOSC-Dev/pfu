//! Abstract syntax tree representation of APML.
//!
//! This AST structure is designed to represent the elements in APML
//! that affect the evaluated result, as a intermdiate representation
//! for [the evaluator][super::eval].
//!
//! This means that optional styling tokens such as spaces and
//! comments are discarded, leaving only a list of variable definitions.
//! Different lexical forms of the same content are also unified with a
//! single superset. For example `"foo"` and `'foo'` will have the same
//! AST representation.
//!
//! ASTs are guaranteed in structure to be grammatically valid,
//! while the evaluation process may fail due to context-dependent
//! constraints such as the `${name:?text}` expansion modifier.
//!
//! [`ApmlAst`] cannot be parsed directly from string. Instead, a parsed LST
//! needs to be [emitted][ApmlAst::emit_from] to get an AST.
//! When emitter gets a LST containing grammatically invalid nodes, for
//! example, a variable definitions followed by another,
//! am [`EmitError`] is produced.
//!
//! On the contrary, AST can be lowered to produce a LST. The LST
//! produced by lowering is also guaranteed to be valid.
//!
//! Although not all LST nodes can be represented in AST form, all AST
//! nodes must have a valid LST form.

use std::{borrow::Cow, cmp::max, num::ParseIntError, rc::Rc};

use thiserror::Error;

use super::{lst, pattern::BashPattern};

/// Trait for AST nodes.
///
/// AST nodes can be emitted from its LST representation or
/// lowered into that.
pub trait AstNode: Sized {
    type LST;

    /// Emits a LST node into AST node.
    fn emit_from(lst: &Self::LST) -> EmitResult<Self>;
    /// Lowers a AST node into LST node.
    fn lower(&self) -> Self::LST;
}

#[derive(Debug, Error)]
pub enum EmitError {
    #[error("Unrepresentable LST node")]
    Unrepresentable,
    #[error("Unparsable integer: {0}")]
    UnparsableInt(#[from] ParseIntError),
    #[error("Missing delimiters between root elements")]
    MissingRootElementDelimiter,
    #[error("Missing delimiters between array elements")]
    MissingArrayElementDelimiter,
}

type EmitResult<T> = std::result::Result<T, EmitError>;

/// A APML abstract syntax tree.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ApmlAst<'a>(pub Vec<VariableDefinition<'a>>);

impl<'a> AstNode for ApmlAst<'a> {
    type LST = lst::ApmlLst<'a>;

    fn emit_from(lst: &Self::LST) -> EmitResult<Self> {
        enum State {
            /// Ready for elements
            Ready,
            /// Needs a delimiter
            NeedDelimiter,
            /// Needs a newline
            NeedNewline,
        }
        let mut state = State::Ready;
        let mut result = Vec::new();
        for token in &lst.0 {
            match token {
                lst::Token::Spacy(_) => {}
                lst::Token::Newline => state = State::Ready,
                lst::Token::Comment(_) => state = State::NeedNewline,
                lst::Token::Variable(def) => {
                    if matches!(state, State::Ready) {
                        result.push(VariableDefinition::emit_from(def)?);
                        state = State::NeedDelimiter;
                    } else {
                        return Err(EmitError::MissingRootElementDelimiter);
                    }
                }
            }
        }
        Ok(Self(result))
    }

    fn lower(&self) -> Self::LST {
        let mut result = Vec::new();
        for def in &self.0 {
            result.push(lst::Token::Variable(def.lower()));
            result.push(lst::Token::Newline);
        }
        result.pop();
        lst::ApmlLst(result)
    }
}

/// A variable definition.
///
/// When emitted from [`lst::VariableDefinition`], the variable operator
/// is omitted. All appending-to operations are desugared. `NAME+="VALUE"`
/// are desugared into `NAME="${NAME}VALUE"` and `NAME+=(VALUES)` are desugared
/// into `NAME=("${NAME[@]}" VALUES)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VariableDefinition<'a> {
    /// Name of the variable.
    pub name: Cow<'a, str>,
    /// Value of the variable.
    pub value: VariableValue<'a>,
}

impl<'a> AstNode for VariableDefinition<'a> {
    type LST = lst::VariableDefinition<'a>;

    fn emit_from(lst: &Self::LST) -> EmitResult<Self> {
        let mut value = VariableValue::emit_from(&lst.value)?;
        match lst.op {
            lst::VariableOp::Assignment => {}
            lst::VariableOp::Append => match &mut value {
                VariableValue::String(text) => {
                    text.0.insert(
                        0,
                        Word::Variable(VariableExpansion {
                            name: lst.name.clone(),
                            modifier: None,
                        }),
                    );
                }
                VariableValue::Array(elements) => {
                    elements.insert(0, ArrayElement::ArrayInclusion(lst.name.clone()));
                }
            },
        }
        Ok(Self {
            name: lst.name.clone(),
            value,
        })
    }

    fn lower(&self) -> Self::LST {
        lst::VariableDefinition {
            name: self.name.clone(),
            op: lst::VariableOp::Assignment,
            value: self.value.lower(),
        }
    }
}

/// A variable value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VariableValue<'a> {
    /// A text value.
    String(Text<'a>),
    /// A array value.
    Array(Vec<ArrayElement<'a>>),
}

impl<'a> AstNode for VariableValue<'a> {
    type LST = lst::VariableValue<'a>;

    fn emit_from(lst: &Self::LST) -> EmitResult<Self> {
        match lst {
            lst::VariableValue::String(text) => Ok(Self::String(Text::emit_from(text)?)),
            lst::VariableValue::Array(tokens) => {
                enum State {
                    /// Ready for elements
                    Ready,
                    /// Needs a delimiter
                    NeedDelimiter,
                    /// Needs a newline
                    NeedNewline,
                }
                let mut state = State::Ready;
                let mut result = Vec::new();
                for token in tokens {
                    match token {
                        lst::ArrayToken::Spacy(_) => {
                            if matches!(state, State::NeedDelimiter | State::Ready) {
                                state = State::Ready;
                            }
                        }
                        lst::ArrayToken::Newline => state = State::Ready,
                        lst::ArrayToken::Comment(_) => state = State::NeedNewline,
                        lst::ArrayToken::Element(_) => {
                            if matches!(state, State::Ready) {
                                result.push(ArrayElement::emit_from(token)?);
                                state = State::NeedDelimiter;
                            } else {
                                return Err(EmitError::MissingArrayElementDelimiter);
                            }
                        }
                    }
                }
                Ok(Self::Array(result))
            }
        }
    }

    fn lower(&self) -> Self::LST {
        match self {
            VariableValue::String(text) => lst::VariableValue::String(Rc::new(text.lower())),
            VariableValue::Array(elements) => {
                let mut result = Vec::new();
                for element in elements {
                    result.push(element.lower());
                    result.push(lst::ArrayToken::Spacy(' '));
                }
                result.pop();
                lst::VariableValue::Array(result)
            }
        }
    }
}

/// A text made by a list of [`Word`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Text<'a>(pub Vec<Word<'a>>);

impl<'a> AstNode for Text<'a> {
    type LST = lst::Text<'a>;

    fn emit_from(lst: &Self::LST) -> EmitResult<Self> {
        let mut result = Vec::new();
        for unit in &lst.0 {
            result.append(&mut emit_text_unit(unit)?);
        }
        Ok(Self(result))
    }

    fn lower(&self) -> Self::LST {
        lst::Text(vec![lst::TextUnit::DoubleQuote(
            self.0.iter().map(Word::lower).collect(),
        )])
    }
}

/// Emits a LST literal string part as string.
fn emit_text_unit<'a>(lst: &lst::TextUnit<'a>) -> EmitResult<Vec<Word<'a>>> {
    match lst {
        lst::TextUnit::Unquoted(words) | lst::TextUnit::DoubleQuote(words) => {
            let mut result = Vec::new();
            for word in words {
                result.push(Word::emit_from(word)?);
            }
            Ok(result)
        }
        lst::TextUnit::SingleQuote(text) => Ok(vec![Word::Literal(text.clone())]),
    }
}

/// A word is a part of a text.
///
/// When emitted from [`lst::Word`], the subcommand variant is emitted as a literal,
/// literal strings are concatenated as one string, and unbraced and braced variable expansions
/// are unified.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Word<'a> {
    /// A literal string.
    Literal(Cow<'a, str>),
    /// A variable expansion.
    Variable(VariableExpansion<'a>),
    /// A complete subcommand string, including `$(` and `)`.
    ///
    /// The inner string is escaped.
    Subcommand(Cow<'a, str>),
}

impl<'a> AstNode for Word<'a> {
    type LST = lst::Word<'a>;

    fn emit_from(lst: &Self::LST) -> EmitResult<Self> {
        match lst {
            lst::Word::Literal(parts) => {
                if parts.len() == 1 {
                    match &parts[0] {
                        lst::LiteralPart::String(str) => Ok(Self::Literal(str.clone())),
                        lst::LiteralPart::Escaped(ch) => Ok(Self::Literal(ch.to_string().into())),
                        lst::LiteralPart::LineContinuation => Ok(Self::Literal(Cow::Borrowed(""))),
                    }
                } else {
                    let mut result = String::new();
                    for part in parts {
                        if matches!(part, lst::LiteralPart::LineContinuation) {
                            // skip LC nodes to avoid emitting useless nodes
                            continue;
                        }
                        emit_literal_part(part, &mut result);
                    }
                    Ok(Self::Literal(result.into()))
                }
            }
            lst::Word::UnbracedVariable(name) => Ok(Self::Variable(VariableExpansion {
                name: name.clone(),
                modifier: None,
            })),
            lst::Word::BracedVariable(expansion) => {
                Ok(Self::Variable(VariableExpansion::emit_from(expansion)?))
            }
            lst::Word::Subcommand(_) => Ok(Self::Subcommand(lst.to_string().into())),
        }
    }

    fn lower(&self) -> Self::LST {
        match self {
            Word::Literal(text) => lst::Word::Literal(lst::LiteralPart::escape(text)),
            Word::Variable(expansion) => lst::Word::BracedVariable(expansion.lower()),
            Word::Subcommand(text) => {
                lst::Word::Literal(vec![lst::LiteralPart::String(text.clone())])
            }
        }
    }
}

/// Emits a LST literal string part as string.
fn emit_literal_part(lst: &lst::LiteralPart, result: &mut String) {
    match lst {
        lst::LiteralPart::String(text) => result.push_str(text),
        lst::LiteralPart::Escaped(ch) => result.push(*ch),
        lst::LiteralPart::LineContinuation => {}
    }
}

/// A variable expansion.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VariableExpansion<'a> {
    /// Name of the variable.
    pub name: Cow<'a, str>,
    /// Modifier to apply to the expanded value.
    pub modifier: Option<ExpansionModifier<'a>>,
}

impl<'a> AstNode for VariableExpansion<'a> {
    type LST = lst::BracedExpansion<'a>;

    fn emit_from(lst: &Self::LST) -> EmitResult<Self> {
        let modifier = lst.modifier.as_ref().take_if(|modifier| {
            !matches!(
                modifier,
                lst::ExpansionModifier::SingleWordElements | lst::ExpansionModifier::ArrayElements
            )
        });
        let modifier = if let Some(modifier) = modifier {
            Some(ExpansionModifier::emit_from(modifier)?)
        } else {
            None
        };
        Ok(Self {
            name: lst.name.clone(),
            modifier,
        })
    }

    fn lower(&self) -> Self::LST {
        lst::BracedExpansion {
            name: self.name.clone(),
            modifier: self.modifier.as_ref().map(AstNode::lower),
        }
    }
}

/// A modifier in the variable expansion.
///
/// When emitting from LST, `SingleWordElements` is cannot be emitted as
/// `${NAME[*]}` is the same as `${NAME}` and the caller should discard the modifier.
///
/// `ArrayElements` is also unrepresentable and should be discarded.
/// In strings, it should be the same as no modifier is provided.
/// In array, it should be emitted as [`ArrayElement::ArrayInclusion`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExpansionModifier<'a> {
    /// Reference to a substring.
    ///
    /// The range is [offset, (offset+length)) (indexing from zero).
    /// If the length is negative, the range is [offset, total+length].
    Substring {
        /// Offset.
        offset: usize,
        /// Length.
        length: Option<isize>,
    },
    /// Stripping the shortest matching prefix.
    StripShortestPrefix(Rc<BashPattern<'a>>),
    /// Stripping the longest matching prefix.
    StripLongestPrefix(Rc<BashPattern<'a>>),
    /// Stripping the shortest matching suffix.
    StripShortestSuffix(Rc<BashPattern<'a>>),
    /// Stripping the longest matching suffix.
    StripLongestSuffix(Rc<BashPattern<'a>>),
    /// Replacing the first match of a pattern with a text.
    ReplaceOnce {
        pattern: Rc<BashPattern<'a>>,
        string: Rc<Text<'a>>,
    },
    /// Replacing the all matches of a pattern with a text.
    ReplaceAll {
        pattern: Rc<BashPattern<'a>>,
        string: Rc<Text<'a>>,
    },
    /// Replacing the prefix of a pattern with a text.
    ReplacePrefix {
        pattern: Rc<BashPattern<'a>>,
        string: Rc<Text<'a>>,
    },
    /// Replacing the suffix of a pattern with a text.
    ReplaceSuffix {
        pattern: Rc<BashPattern<'a>>,
        string: Rc<Text<'a>>,
    },
    /// Upper-casify the first match of a pattern.
    UpperOnce(Rc<BashPattern<'a>>),
    /// Upper-casify the all matches of a pattern.
    UpperAll(Rc<BashPattern<'a>>),
    /// Lower-casify the first match of a pattern.
    LowerOnce(Rc<BashPattern<'a>>),
    /// Lower-casify the all matches of a pattern.
    LowerAll(Rc<BashPattern<'a>>),
    /// Producing errors when the variable is unset or null.
    ErrorOnUnset(Rc<Text<'a>>),
    /// Returning the length of the variable.
    Length,
    /// Returning a text when the variable is unset or null.
    WhenUnset(Rc<Text<'a>>),
    /// Returning a text when the variable is set.
    WhenSet(Rc<Text<'a>>),
}

impl<'a> AstNode for ExpansionModifier<'a> {
    type LST = lst::ExpansionModifier<'a>;

    fn emit_from(lst: &Self::LST) -> EmitResult<Self> {
        match lst {
            lst::ExpansionModifier::Substring { offset, length } => Ok(Self::Substring {
                offset: max(offset.as_ref().trim().parse::<isize>()?, 0) as usize,
                length: if let Some(length) = length {
                    Some(length.as_ref().trim().parse::<isize>()?)
                } else {
                    None
                },
            }),
            lst::ExpansionModifier::StripShortestPrefix(pattern) => {
                Ok(Self::StripShortestPrefix(pattern.clone()))
            }
            lst::ExpansionModifier::StripLongestPrefix(pattern) => {
                Ok(Self::StripLongestPrefix(pattern.clone()))
            }
            lst::ExpansionModifier::StripShortestSuffix(pattern) => {
                Ok(Self::StripShortestSuffix(pattern.clone()))
            }
            lst::ExpansionModifier::StripLongestSuffix(pattern) => {
                Ok(Self::StripLongestSuffix(pattern.clone()))
            }
            lst::ExpansionModifier::ReplaceOnce { pattern, string } => Ok(Self::ReplaceOnce {
                pattern: pattern.clone(),
                string: Rc::new(if let Some(text) = string {
                    Text::emit_from(text)?
                } else {
                    Text::default()
                }),
            }),
            lst::ExpansionModifier::ReplaceAll { pattern, string } => Ok(Self::ReplaceAll {
                pattern: pattern.clone(),
                string: Rc::new(if let Some(text) = string {
                    Text::emit_from(text)?
                } else {
                    Text::default()
                }),
            }),
            lst::ExpansionModifier::ReplacePrefix { pattern, string } => Ok(Self::ReplacePrefix {
                pattern: pattern.clone(),
                string: Rc::new(if let Some(text) = string {
                    Text::emit_from(text)?
                } else {
                    Text::default()
                }),
            }),
            lst::ExpansionModifier::ReplaceSuffix { pattern, string } => Ok(Self::ReplaceSuffix {
                pattern: pattern.clone(),
                string: Rc::new(if let Some(text) = string {
                    Text::emit_from(text)?
                } else {
                    Text::default()
                }),
            }),
            lst::ExpansionModifier::UpperOnce(pattern) => Ok(Self::UpperOnce(pattern.clone())),
            lst::ExpansionModifier::UpperAll(pattern) => Ok(Self::UpperAll(pattern.clone())),
            lst::ExpansionModifier::LowerOnce(pattern) => Ok(Self::LowerOnce(pattern.clone())),
            lst::ExpansionModifier::LowerAll(pattern) => Ok(Self::LowerAll(pattern.clone())),
            lst::ExpansionModifier::ErrorOnUnset(text) => {
                Ok(Self::ErrorOnUnset(Rc::new(Text::emit_from(text)?)))
            }
            lst::ExpansionModifier::Length => Ok(Self::Length),
            lst::ExpansionModifier::WhenUnset(text) => {
                Ok(Self::WhenUnset(Rc::new(Text::emit_from(text)?)))
            }
            lst::ExpansionModifier::WhenSet(text) => {
                Ok(Self::WhenSet(Rc::new(Text::emit_from(text)?)))
            }
            lst::ExpansionModifier::ArrayElements => Err(EmitError::Unrepresentable),
            lst::ExpansionModifier::SingleWordElements => Err(EmitError::Unrepresentable),
        }
    }

    fn lower(&self) -> Self::LST {
        match self {
            ExpansionModifier::Substring { offset, length } => lst::ExpansionModifier::Substring {
                offset: offset.to_string().into(),
                length: length.map(|length| length.to_string().into()),
            },
            ExpansionModifier::StripShortestPrefix(pattern) => {
                lst::ExpansionModifier::StripShortestPrefix(pattern.clone())
            }
            ExpansionModifier::StripLongestPrefix(pattern) => {
                lst::ExpansionModifier::StripLongestPrefix(pattern.clone())
            }
            ExpansionModifier::StripShortestSuffix(pattern) => {
                lst::ExpansionModifier::StripShortestSuffix(pattern.clone())
            }
            ExpansionModifier::StripLongestSuffix(pattern) => {
                lst::ExpansionModifier::StripLongestSuffix(pattern.clone())
            }
            ExpansionModifier::ReplaceOnce { pattern, string } => {
                lst::ExpansionModifier::ReplaceOnce {
                    pattern: pattern.clone(),
                    string: Some(Rc::new(string.lower())),
                }
            }
            ExpansionModifier::ReplaceAll { pattern, string } => {
                lst::ExpansionModifier::ReplaceAll {
                    pattern: pattern.clone(),
                    string: Some(Rc::new(string.lower())),
                }
            }
            ExpansionModifier::ReplacePrefix { pattern, string } => {
                lst::ExpansionModifier::ReplacePrefix {
                    pattern: pattern.clone(),
                    string: Some(Rc::new(string.lower())),
                }
            }
            ExpansionModifier::ReplaceSuffix { pattern, string } => {
                lst::ExpansionModifier::ReplaceSuffix {
                    pattern: pattern.clone(),
                    string: Some(Rc::new(string.lower())),
                }
            }
            ExpansionModifier::UpperOnce(pattern) => {
                lst::ExpansionModifier::UpperOnce(pattern.clone())
            }
            ExpansionModifier::UpperAll(pattern) => {
                lst::ExpansionModifier::UpperAll(pattern.clone())
            }
            ExpansionModifier::LowerOnce(pattern) => {
                lst::ExpansionModifier::LowerOnce(pattern.clone())
            }
            ExpansionModifier::LowerAll(pattern) => {
                lst::ExpansionModifier::LowerAll(pattern.clone())
            }
            ExpansionModifier::ErrorOnUnset(text) => {
                lst::ExpansionModifier::ErrorOnUnset(Rc::new(text.lower()))
            }
            ExpansionModifier::Length => lst::ExpansionModifier::Length,
            ExpansionModifier::WhenUnset(text) => {
                lst::ExpansionModifier::WhenUnset(Rc::new(text.lower()))
            }
            ExpansionModifier::WhenSet(text) => {
                lst::ExpansionModifier::WhenSet(Rc::new(text.lower()))
            }
        }
    }
}

/// A element of an array.
///
/// Spacy tokens, newline and comments are discarded
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ArrayElement<'a> {
    /// A element expanding to all elements of another array.
    ArrayInclusion(Cow<'a, str>),
    /// A text element.
    Text(Rc<Text<'a>>),
}

impl<'a> AstNode for ArrayElement<'a> {
    type LST = lst::ArrayToken<'a>;

    fn emit_from(lst: &Self::LST) -> EmitResult<Self> {
        match lst {
            lst::ArrayToken::Spacy(_) | lst::ArrayToken::Newline | lst::ArrayToken::Comment(_) => {
                Err(EmitError::Unrepresentable)
            }
            lst::ArrayToken::Element(text) => {
                let units = &text.0;
                if units.len() == 1 {
                    let unit = &units[0];
                    match unit {
                        lst::TextUnit::Unquoted(words) | lst::TextUnit::DoubleQuote(words) => {
                            if words.len() == 1 {
                                let word = &words[0];
                                if let lst::Word::BracedVariable(word) = word {
                                    if word.modifier == Some(lst::ExpansionModifier::ArrayElements)
                                    {
                                        // expand array elements
                                        return Ok(Self::ArrayInclusion(word.name.clone()));
                                    }
                                }
                            }
                        }
                        lst::TextUnit::SingleQuote(_) => {}
                    }
                }
                Ok(Self::Text(Rc::new(Text::emit_from(text)?)))
            }
        }
    }

    fn lower(&self) -> Self::LST {
        match self {
            ArrayElement::ArrayInclusion(name) => {
                lst::ArrayToken::Element(Rc::new(lst::Text(vec![lst::TextUnit::DoubleQuote(
                    vec![lst::Word::BracedVariable(lst::BracedExpansion {
                        name: name.clone(),
                        modifier: Some(lst::ExpansionModifier::ArrayElements),
                    })],
                )])))
            }
            ArrayElement::Text(text) => lst::ArrayToken::Element(Rc::new(text.lower())),
        }
    }
}

#[cfg(test)]
mod test {
    use std::fmt::Debug;

    use crate::apml::pattern::GlobPart;

    use super::*;

    fn assert_emit_fail<AST: AstNode<LST = LST> + Debug, LST: ToString>(source: LST) {
        AST::emit_from(&source).unwrap_err();
    }

    fn assert_emit_lower<AST: AstNode<LST = LST> + Eq + Debug, LST: ToString>(
        source: LST,
        ast: AST,
        lst: &str,
    ) {
        let result = AST::emit_from(&source).unwrap();
        assert_eq!(result, ast);
        assert_eq!(result.lower().to_string(), lst);
    }

    #[test]
    fn test_apml_ast() {
        let text_lst = Rc::new(lst::Text(vec![lst::TextUnit::SingleQuote("foo$\\".into())]));
        let text_ast = Text(vec![Word::Literal("foo$\\".into())]);
        let def_lst = lst::VariableDefinition {
            name: "test".into(),
            op: lst::VariableOp::Assignment,
            value: lst::VariableValue::String(text_lst.clone()),
        };
        let def_ast = VariableDefinition {
            name: "test".into(),
            value: VariableValue::String(text_ast.clone()),
        };
        assert_emit_lower(
            lst::ApmlLst(vec![
                lst::Token::Variable(def_lst.clone()),
                lst::Token::Newline,
                lst::Token::Spacy(' '),
                lst::Token::Variable(def_lst.clone()),
                lst::Token::Comment("a".into()),
                lst::Token::Newline,
                lst::Token::Variable(def_lst.clone()),
            ]),
            ApmlAst(vec![def_ast.clone(), def_ast.clone(), def_ast.clone()]),
            "test=\"foo\\$\\\\\"\ntest=\"foo\\$\\\\\"\ntest=\"foo\\$\\\\\"",
        );
        assert_emit_fail::<ApmlAst, _>(lst::ApmlLst(vec![
            lst::Token::Variable(def_lst.clone()),
            lst::Token::Variable(def_lst.clone()),
        ]));
        assert_emit_fail::<ApmlAst, _>(lst::ApmlLst(vec![
            lst::Token::Variable(def_lst.clone()),
            lst::Token::Spacy(' '),
            lst::Token::Variable(def_lst.clone()),
        ]));
        assert_emit_fail::<ApmlAst, _>(lst::ApmlLst(vec![
            lst::Token::Variable(def_lst.clone()),
            lst::Token::Comment("a".into()),
            lst::Token::Spacy(' '),
            lst::Token::Variable(def_lst.clone()),
        ]));
    }

    #[test]
    fn test_variable_definition() {
        let text_lst = Rc::new(lst::Text(vec![lst::TextUnit::SingleQuote("foo$\\".into())]));
        let text_ast = Text(vec![Word::Literal("foo$\\".into())]);
        assert_emit_lower(
            lst::VariableDefinition {
                name: "test".into(),
                op: lst::VariableOp::Assignment,
                value: lst::VariableValue::String(text_lst.clone()),
            },
            VariableDefinition {
                name: "test".into(),
                value: VariableValue::String(text_ast.clone()),
            },
            "test=\"foo\\$\\\\\"",
        );
        assert_emit_lower(
            lst::VariableDefinition {
                name: "test".into(),
                op: lst::VariableOp::Append,
                value: lst::VariableValue::String(text_lst.clone()),
            },
            VariableDefinition {
                name: "test".into(),
                value: VariableValue::String(Text(vec![
                    Word::Variable(VariableExpansion {
                        name: "test".into(),
                        modifier: None,
                    }),
                    Word::Literal("foo$\\".into()),
                ])),
            },
            "test=\"${test}foo\\$\\\\\"",
        );
        assert_emit_lower(
            lst::VariableDefinition {
                name: "test".into(),
                op: lst::VariableOp::Assignment,
                value: lst::VariableValue::Array(vec![lst::ArrayToken::Element(text_lst.clone())]),
            },
            VariableDefinition {
                name: "test".into(),
                value: VariableValue::Array(vec![ArrayElement::Text(Rc::new(text_ast.clone()))]),
            },
            "test=(\"foo\\$\\\\\")",
        );
        assert_emit_lower(
            lst::VariableDefinition {
                name: "test".into(),
                op: lst::VariableOp::Append,
                value: lst::VariableValue::Array(vec![lst::ArrayToken::Element(text_lst.clone())]),
            },
            VariableDefinition {
                name: "test".into(),
                value: VariableValue::Array(vec![
                    ArrayElement::ArrayInclusion("test".into()),
                    ArrayElement::Text(Rc::new(text_ast.clone())),
                ]),
            },
            "test=(\"${test[@]}\" \"foo\\$\\\\\")",
        );
    }

    #[test]
    fn test_variable_value() {
        let text_lst = Rc::new(lst::Text(vec![lst::TextUnit::SingleQuote("foo$\\".into())]));
        let text_ast = Text(vec![Word::Literal("foo$\\".into())]);
        assert_emit_lower(
            lst::VariableValue::String(text_lst.clone()),
            VariableValue::String(text_ast.clone()),
            "\"foo\\$\\\\\"",
        );
        assert_emit_lower(
            lst::VariableValue::Array(vec![
                lst::ArrayToken::Element(text_lst.clone()),
                lst::ArrayToken::Spacy(' '),
                lst::ArrayToken::Element(text_lst.clone()),
                lst::ArrayToken::Comment("a".into()),
                lst::ArrayToken::Newline,
                lst::ArrayToken::Element(text_lst.clone()),
            ]),
            VariableValue::Array(vec![
                ArrayElement::Text(Rc::new(text_ast.clone())),
                ArrayElement::Text(Rc::new(text_ast.clone())),
                ArrayElement::Text(Rc::new(text_ast.clone())),
            ]),
            "(\"foo\\$\\\\\" \"foo\\$\\\\\" \"foo\\$\\\\\")",
        );
        assert_emit_fail::<VariableValue, _>(lst::VariableValue::Array(vec![
            lst::ArrayToken::Element(text_lst.clone()),
            lst::ArrayToken::Element(text_lst.clone()),
        ]));
        assert_emit_fail::<VariableValue, _>(lst::VariableValue::Array(vec![
            lst::ArrayToken::Element(text_lst.clone()),
            lst::ArrayToken::Comment("a".into()),
            lst::ArrayToken::Spacy(' '),
            lst::ArrayToken::Element(text_lst.clone()),
        ]));
    }

    #[test]
    fn test_text() {
        assert_emit_lower(
            lst::Text(vec![
                lst::TextUnit::SingleQuote("test".into()),
                lst::TextUnit::DoubleQuote(vec![lst::Word::Literal(lst::LiteralPart::escape(
                    "test$$",
                ))]),
            ]),
            Text(vec![
                Word::Literal("test".into()),
                Word::Literal("test$$".into()),
            ]),
            "\"testtest\\$\\$\"",
        );
    }

    #[test]
    fn test_word() {
        assert_emit_lower(
            lst::Word::Literal(lst::LiteralPart::escape("test$$")),
            Word::Literal("test$$".into()),
            "test\\$\\$",
        );
        assert_emit_lower(
            lst::Word::UnbracedVariable("a".into()),
            Word::Variable(VariableExpansion {
                name: "a".into(),
                modifier: None,
            }),
            "${a}",
        );
        assert_emit_lower(
            lst::Word::BracedVariable(lst::BracedExpansion {
                name: "a".into(),
                modifier: None,
            }),
            Word::Variable(VariableExpansion {
                name: "a".into(),
                modifier: None,
            }),
            "${a}",
        );
        assert_emit_lower(
            lst::Word::Subcommand(vec![
                lst::ArrayToken::Element(Rc::new(lst::Text(vec![lst::TextUnit::SingleQuote(
                    "true".into(),
                )]))),
                lst::ArrayToken::Spacy(' '),
                lst::ArrayToken::Element(Rc::new(lst::Text(vec![lst::TextUnit::DoubleQuote(
                    vec![
                        lst::Word::Literal(vec![lst::LiteralPart::String("foo$\\".into())]),
                        lst::Word::UnbracedVariable("asdf".into()),
                    ],
                )]))),
            ]),
            Word::Subcommand("$('true' \"foo$\\$asdf\")".into()),
            "$('true' \"foo$\\$asdf\")",
        );
        assert_emit_lower(
            lst::Word::Literal(lst::LiteralPart::escape("test$$\n")),
            Word::Literal("test$$\n".into()),
            "test\\$\\$\n",
        );
        assert_emit_lower(
            lst::Word::Literal(vec![
                lst::LiteralPart::String("test".into()),
                lst::LiteralPart::LineContinuation,
                lst::LiteralPart::String("test\ntest".into()),
            ]),
            Word::Literal("testtest\ntest".into()),
            "testtest\ntest",
        );
    }

    #[test]
    fn test_variable_expansion() {
        assert_emit_lower(
            lst::BracedExpansion {
                name: "test".into(),
                modifier: None,
            },
            VariableExpansion {
                name: "test".into(),
                modifier: None,
            },
            "test",
        );
        assert_emit_lower(
            lst::BracedExpansion {
                name: "test".into(),
                modifier: Some(lst::ExpansionModifier::Length),
            },
            VariableExpansion {
                name: "test".into(),
                modifier: Some(ExpansionModifier::Length),
            },
            "#test",
        );
        assert_emit_lower(
            lst::BracedExpansion {
                name: "test".into(),
                modifier: Some(lst::ExpansionModifier::ArrayElements),
            },
            VariableExpansion {
                name: "test".into(),
                modifier: None,
            },
            "test",
        );
        assert_emit_lower(
            lst::BracedExpansion {
                name: "test".into(),
                modifier: Some(lst::ExpansionModifier::SingleWordElements),
            },
            VariableExpansion {
                name: "test".into(),
                modifier: None,
            },
            "test",
        );
    }

    #[test]
    fn test_expansion_modifier() {
        let pattern = Rc::new(BashPattern(vec![
            GlobPart::String("1a".into()),
            GlobPart::AnyString,
        ]));
        let text_lst = Rc::new(lst::Text(vec![lst::TextUnit::SingleQuote("foo$\\".into())]));
        let text_ast = Rc::new(Text(vec![Word::Literal("foo$\\".into())]));
        assert_emit_lower(
            lst::ExpansionModifier::Substring {
                offset: "10".into(),
                length: None,
            },
            ExpansionModifier::Substring {
                offset: 10,
                length: None,
            },
            ":10",
        );
        assert_emit_fail::<ExpansionModifier, _>(lst::ExpansionModifier::Substring {
            offset: "".into(),
            length: None,
        });
        assert_emit_lower(
            lst::ExpansionModifier::Substring {
                offset: "-1".into(),
                length: None,
            },
            ExpansionModifier::Substring {
                offset: 0,
                length: None,
            },
            ":0",
        );
        assert_emit_lower(
            lst::ExpansionModifier::Substring {
                offset: "0".into(),
                length: Some("-10".into()),
            },
            ExpansionModifier::Substring {
                offset: 0,
                length: Some(-10),
            },
            ":0:-10",
        );
        assert_emit_lower(
            lst::ExpansionModifier::StripShortestPrefix(pattern.clone()),
            ExpansionModifier::StripShortestPrefix(pattern.clone()),
            "#1a*",
        );
        assert_emit_lower(
            lst::ExpansionModifier::StripLongestPrefix(pattern.clone()),
            ExpansionModifier::StripLongestPrefix(pattern.clone()),
            "##1a*",
        );
        assert_emit_lower(
            lst::ExpansionModifier::StripShortestSuffix(pattern.clone()),
            ExpansionModifier::StripShortestSuffix(pattern.clone()),
            "%1a*",
        );
        assert_emit_lower(
            lst::ExpansionModifier::StripLongestSuffix(pattern.clone()),
            ExpansionModifier::StripLongestSuffix(pattern.clone()),
            "%%1a*",
        );
        assert_emit_lower(
            lst::ExpansionModifier::ReplaceOnce {
                pattern: pattern.clone(),
                string: None,
            },
            ExpansionModifier::ReplaceOnce {
                pattern: pattern.clone(),
                string: Rc::new(Text::default()),
            },
            "/1a*/\"\"",
        );
        assert_emit_lower(
            lst::ExpansionModifier::ReplaceOnce {
                pattern: pattern.clone(),
                string: Some(text_lst.clone()),
            },
            ExpansionModifier::ReplaceOnce {
                pattern: pattern.clone(),
                string: text_ast.clone(),
            },
            "/1a*/\"foo\\$\\\\\"",
        );
        assert_emit_lower(
            lst::ExpansionModifier::ReplaceAll {
                pattern: pattern.clone(),
                string: None,
            },
            ExpansionModifier::ReplaceAll {
                pattern: pattern.clone(),
                string: Rc::new(Text::default()),
            },
            "//1a*/\"\"",
        );
        assert_emit_lower(
            lst::ExpansionModifier::ReplaceAll {
                pattern: pattern.clone(),
                string: Some(text_lst.clone()),
            },
            ExpansionModifier::ReplaceAll {
                pattern: pattern.clone(),
                string: text_ast.clone(),
            },
            "//1a*/\"foo\\$\\\\\"",
        );
        assert_emit_lower(
            lst::ExpansionModifier::ReplacePrefix {
                pattern: pattern.clone(),
                string: None,
            },
            ExpansionModifier::ReplacePrefix {
                pattern: pattern.clone(),
                string: Rc::new(Text::default()),
            },
            "/#1a*/\"\"",
        );
        assert_emit_lower(
            lst::ExpansionModifier::ReplacePrefix {
                pattern: pattern.clone(),
                string: Some(text_lst.clone()),
            },
            ExpansionModifier::ReplacePrefix {
                pattern: pattern.clone(),
                string: text_ast.clone(),
            },
            "/#1a*/\"foo\\$\\\\\"",
        );
        assert_emit_lower(
            lst::ExpansionModifier::ReplaceSuffix {
                pattern: pattern.clone(),
                string: None,
            },
            ExpansionModifier::ReplaceSuffix {
                pattern: pattern.clone(),
                string: Rc::new(Text::default()),
            },
            "/%1a*/\"\"",
        );
        assert_emit_lower(
            lst::ExpansionModifier::ReplaceSuffix {
                pattern: pattern.clone(),
                string: Some(text_lst.clone()),
            },
            ExpansionModifier::ReplaceSuffix {
                pattern: pattern.clone(),
                string: text_ast.clone(),
            },
            "/%1a*/\"foo\\$\\\\\"",
        );
        assert_emit_lower(
            lst::ExpansionModifier::UpperOnce(pattern.clone()),
            ExpansionModifier::UpperOnce(pattern.clone()),
            "^1a*",
        );
        assert_emit_lower(
            lst::ExpansionModifier::UpperAll(pattern.clone()),
            ExpansionModifier::UpperAll(pattern.clone()),
            "^^1a*",
        );
        assert_emit_lower(
            lst::ExpansionModifier::LowerOnce(pattern.clone()),
            ExpansionModifier::LowerOnce(pattern.clone()),
            ",1a*",
        );
        assert_emit_lower(
            lst::ExpansionModifier::LowerAll(pattern.clone()),
            ExpansionModifier::LowerAll(pattern.clone()),
            ",,1a*",
        );
        assert_emit_lower(
            lst::ExpansionModifier::ErrorOnUnset(text_lst.clone()),
            ExpansionModifier::ErrorOnUnset(text_ast.clone()),
            ":?\"foo\\$\\\\\"",
        );
        assert_emit_lower(
            lst::ExpansionModifier::Length,
            ExpansionModifier::Length,
            "#",
        );
        assert_emit_lower(
            lst::ExpansionModifier::WhenUnset(text_lst.clone()),
            ExpansionModifier::WhenUnset(text_ast.clone()),
            ":-\"foo\\$\\\\\"",
        );
        assert_emit_lower(
            lst::ExpansionModifier::WhenSet(text_lst.clone()),
            ExpansionModifier::WhenSet(text_ast.clone()),
            ":+\"foo\\$\\\\\"",
        );
        assert_emit_fail::<ExpansionModifier, _>(lst::ExpansionModifier::ArrayElements);
        assert_emit_fail::<ExpansionModifier, _>(lst::ExpansionModifier::SingleWordElements);
    }

    #[test]
    fn test_array_element() {
        assert_emit_lower(
            lst::ArrayToken::Element(Rc::new(lst::Text(vec![lst::TextUnit::DoubleQuote(vec![
                lst::Word::BracedVariable(lst::BracedExpansion {
                    name: "a".into(),
                    modifier: Some(lst::ExpansionModifier::ArrayElements),
                }),
            ])]))),
            ArrayElement::ArrayInclusion("a".into()),
            "\"${a[@]}\"",
        );
        assert_emit_lower(
            lst::ArrayToken::Element(Rc::new(lst::Text(vec![lst::TextUnit::SingleQuote(
                "a".into(),
            )]))),
            ArrayElement::Text(Rc::new(Text(vec![Word::Literal("a".into())]))),
            "\"a\"",
        );
        assert_emit_fail::<ArrayElement, _>(lst::ArrayToken::Spacy(' '));
        assert_emit_fail::<ArrayElement, _>(lst::ArrayToken::Newline);
        assert_emit_fail::<ArrayElement, _>(lst::ArrayToken::Comment("a".into()));
    }
}
