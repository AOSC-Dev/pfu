//! APML lossless syntax tree.
//!
//! This syntax tree is designed to correspond byte by byte
//! to the source file in order to obtain a lossless reverse
//! conversion capability to the source file.

use std::{
    borrow::Cow,
    fmt::{Debug, Display, Write},
    rc::Rc,
};

use super::glob::GlobPattern;

/// A APML parse-tree, consisting of a list of tokens.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ApmlParseTree<'a>(pub Vec<Token<'a>>);

impl Display for ApmlParseTree<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for token in &self.0 {
            Display::fmt(token, f)?;
        }
        Ok(())
    }
}

impl<'a> ApmlParseTree<'a> {
    /// Parses a APML source string into lossless syntax tree.
    pub fn parse(src: &'a str) -> Result<Self, nom::Err<nom::error::Error<&'a str>>> {
        let (src, tree) = super::parser::apml_ast(src)?;
        if !src.is_empty() {
            return Err(nom::Err::Failure(nom::error::make_error(
                src,
                nom::error::ErrorKind::Fail,
            )));
        }
        Ok(tree)
    }
}

/// A token in the AST.
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
    /// Bianry operator.
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
    String(Rc<Text<'a>>),
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
    DuobleQuote(Vec<Word<'a>>),
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
            TextUnit::SingleQuote(text) => f.write_fmt(format_args!("'{}'", text)),
            TextUnit::DuobleQuote(words) => {
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
    /// A literal string (`"<parts>"`)
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
            Word::UnbracedVariable(name) => f.write_fmt(format_args!("${}", name)),
            Word::BracedVariable(exp) => f.write_fmt(format_args!("${{{}}}", exp)),
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
            Some(ExpansionModifier::Length) => f.write_fmt(format_args!("#{}", self.name)),
            None => f.write_str(&self.name),
            Some(modifier) => f.write_fmt(format_args!("{}{}", self.name, modifier)),
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
    StripShortestPrefix(Rc<GlobPattern<'a>>),
    /// Stripping the longest matching prefix (`"##<pattern>"`).
    StripLongestPrefix(Rc<GlobPattern<'a>>),
    /// Stripping the shortest matching suffix (`"%<pattern>"`).
    StripShortestSuffix(Rc<GlobPattern<'a>>),
    /// Stripping the longest matching suffix (`"%%<pattern>"`).
    StripLongestSuffix(Rc<GlobPattern<'a>>),
    /// Replacing the first match of a pattern with a text (`"/<pattern>[/<string>]"`).
    ///
    /// `string` can be ommitted, leaving `"/<pattern>"` structure,
    /// which removes the first match of the pattern.
    ReplaceOnce {
        pattern: Rc<GlobPattern<'a>>,
        string: Option<Rc<Text<'a>>>,
    },
    /// Replacing the all matches of a pattern with a text (`"//<pattern>[/<string>]"`).
    ///
    /// `string` can be ommitted.
    ReplaceAll {
        pattern: Rc<GlobPattern<'a>>,
        string: Option<Rc<Text<'a>>>,
    },
    /// Replacing the prefix of a pattern with a text (`"/#<pattern>[/<string>]"`).
    ///
    /// `string` can be ommitted.
    ReplacePrefix {
        pattern: Rc<GlobPattern<'a>>,
        string: Option<Rc<Text<'a>>>,
    },
    /// Replacing the suffix of a pattern with a text (`"/%<pattern>[/<string>]"`).
    ///
    /// `string` can be ommitted.
    ReplaceSuffix {
        pattern: Rc<GlobPattern<'a>>,
        string: Option<Rc<Text<'a>>>,
    },
    /// Upper-casify the first match of a pattern (`"^<pattern>"`).
    UpperOnce(Rc<GlobPattern<'a>>),
    /// Upper-casify the all matches of a pattern (`"^^<pattern>"`).
    UpperAll(Rc<GlobPattern<'a>>),
    /// Lower-casify the first match of a pattern (`",<pattern>"`).
    LowerOnce(Rc<GlobPattern<'a>>),
    /// Lower-casify the all matches of a pattern (`",,<pattern>"`).
    LowerAll(Rc<GlobPattern<'a>>),
    /// Producing errors when the variable is unset or null (`":?<text>"`).
    ErrorOnUnset(Rc<Text<'a>>),
    /// Returning the length of the variable.
    ///
    /// Note that this modifier uses a special format, see [BracedExpansion].
    Length,
    /// Returning a text when the variable is unset or null (`":-<text>"`).
    WhenUnset(Rc<Text<'a>>),
    /// Returning a text when the variable is set (`":+<text>"`).
    WhenSet(Rc<Text<'a>>),
    /// Expands to array elements (`"[@]"`).
    ArrayElements,
    /// Expands to a string of array elements concatened with space (`"[*]"`).
    SingleWordElements,
}

impl Display for ExpansionModifier<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExpansionModifier::Substring { offset, length } => match length {
                None => f.write_fmt(format_args!(":{}", offset)),
                Some(length) => f.write_fmt(format_args!(":{}:{}", offset, length)),
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
            ExpansionModifier::ReplaceOnce { pattern, string } => match string {
                Some(string) => f.write_fmt(format_args!("/{}/{}", pattern, string)),
                None => f.write_fmt(format_args!("/{}", pattern)),
            },
            ExpansionModifier::ReplaceAll { pattern, string } => match string {
                Some(string) => f.write_fmt(format_args!("//{}/{}", pattern, string)),
                None => f.write_fmt(format_args!("//{}", pattern)),
            },
            ExpansionModifier::ReplacePrefix { pattern, string } => match string {
                Some(string) => f.write_fmt(format_args!("/#{}/{}", pattern, string)),
                None => f.write_fmt(format_args!("/#{}", pattern)),
            },
            ExpansionModifier::ReplaceSuffix { pattern, string } => match string {
                Some(string) => f.write_fmt(format_args!("/%{}/{}", pattern, string)),
                None => f.write_fmt(format_args!("/%{}", pattern)),
            },
            ExpansionModifier::UpperOnce(pattern) => f.write_fmt(format_args!("^{}", pattern)),
            ExpansionModifier::UpperAll(pattern) => f.write_fmt(format_args!("^^{}", pattern)),
            ExpansionModifier::LowerOnce(pattern) => f.write_fmt(format_args!(",{}", pattern)),
            ExpansionModifier::LowerAll(pattern) => f.write_fmt(format_args!(",,{}", pattern)),
            ExpansionModifier::ErrorOnUnset(text) => f.write_fmt(format_args!(":?{}", text)),
            ExpansionModifier::Length => unreachable!(),
            ExpansionModifier::WhenUnset(text) => f.write_fmt(format_args!(":-{}", text)),
            ExpansionModifier::WhenSet(text) => f.write_fmt(format_args!(":+{}", text)),
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
    Element(Rc<Text<'a>>),
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
