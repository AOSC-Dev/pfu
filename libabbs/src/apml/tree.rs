//! APML parse-tree.
//!
//! This AST structure is designed to correspond byte by byte
//! to the source file in order to obtain a complete reverse
//! conversion capability to the source file.

use std::{borrow::Cow, rc::Rc};

/// A APML parse-tree, consisting of a list of tokens.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ApmlParseTree<'a>(pub Vec<Token<'a>>);

impl ToString for ApmlParseTree<'_> {
    fn to_string(&self) -> String {
        self.0
            .iter()
            .map(|token| token.to_string())
            .collect::<Vec<_>>()
            .join("")
    }
}

impl<'a> ApmlParseTree<'a> {
    pub fn parse(src: &'a str) -> Result<Self, nom::Err<nom::error::Error<&'a str>>> {
        let (src, tree) = super::parser::apml_ast(src)?;
        if src.len() != 0 {
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

impl ToString for Token<'_> {
    fn to_string(&self) -> String {
        match self {
            Token::Spacy(ch) => ch.to_string(),
            Token::Newline => "\n".to_string(),
            Token::Comment(text) => format!("#{}", text),
            Token::Variable(def) => def.to_string(),
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

impl ToString for VariableDefinition<'_> {
    fn to_string(&self) -> String {
        format!(
            "{}{}{}",
            self.name,
            self.op.to_string(),
            self.value.to_string()
        )
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

impl ToString for VariableOp {
    fn to_string(&self) -> String {
        match self {
            VariableOp::Assignment => "=".to_string(),
            VariableOp::Append => "+=".to_string(),
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

impl ToString for VariableValue<'_> {
    fn to_string(&self) -> String {
        match self {
            VariableValue::String(text) => text.to_string(),
            VariableValue::Array(tokens) => format!(
                "({})",
                tokens
                    .iter()
                    .map(|token| token.to_string())
                    .collect::<Vec<_>>()
                    .join("")
            ),
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

impl ToString for Text<'_> {
    fn to_string(&self) -> String {
        self.0
            .iter()
            .map(|unit| unit.to_string())
            .collect::<Vec<_>>()
            .join("")
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

impl ToString for TextUnit<'_> {
    fn to_string(&self) -> String {
        match self {
            TextUnit::Unquoted(words) => words
                .iter()
                .map(|word| word.to_string())
                .collect::<Vec<_>>()
                .join(""),
            TextUnit::SingleQuote(text) => format!("'{}'", text),
            TextUnit::DuobleQuote(words) => format!(
                "\"{}\"",
                words
                    .iter()
                    .map(|word| word.to_string())
                    .collect::<Vec<_>>()
                    .join("")
            ),
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

impl ToString for Word<'_> {
    fn to_string(&self) -> String {
        match self {
            Word::Literal(parts) => parts
                .iter()
                .map(|part| part.to_string())
                .collect::<Vec<_>>()
                .join(""),
            Word::UnbracedVariable(name) => format!("${}", name),
            Word::BracedVariable(exp) => format!("${{{}}}", exp.to_string()),
            Word::Subcommand(tokens) => format!(
                "$({})",
                tokens
                    .iter()
                    .map(|token| token.to_string())
                    .collect::<Vec<_>>()
                    .join(""),
            ),
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

impl ToString for LiteralPart<'_> {
    fn to_string(&self) -> String {
        match self {
            LiteralPart::String(text) => text.to_string(),
            LiteralPart::Escaped(ch) => format!("\\{}", ch),
            LiteralPart::LineContinuation => "\\\n".to_string(),
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

impl ToString for BracedExpansion<'_> {
    fn to_string(&self) -> String {
        match &self.modifier {
            Some(ExpansionModifier::Length) => format!("#{}", self.name),
            None => self.name.to_string(),
            Some(modifier) => format!("{}{}", self.name, modifier.to_string()),
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

impl ToString for ExpansionModifier<'_> {
    fn to_string(&self) -> String {
        match self {
            ExpansionModifier::Substring { offset, length } => match length {
                None => format!(":{}", offset),
                Some(length) => format!(":{}:{}", offset, length),
            },
            ExpansionModifier::StripShortestPrefix(pattern) => format!("#{}", pattern.to_string()),
            ExpansionModifier::StripLongestPrefix(pattern) => format!("##{}", pattern.to_string()),
            ExpansionModifier::StripShortestSuffix(pattern) => format!("%{}", pattern.to_string()),
            ExpansionModifier::StripLongestSuffix(pattern) => format!("%%{}", pattern.to_string()),
            ExpansionModifier::ReplaceOnce { pattern, string } => match string {
                Some(string) => format!("/{}/{}", pattern.to_string(), string.to_string()),
                None => format!("/{}", pattern.to_string()),
            },
            ExpansionModifier::ReplaceAll { pattern, string } => match string {
                Some(string) => format!("//{}/{}", pattern.to_string(), string.to_string()),
                None => format!("//{}", pattern.to_string()),
            },
            ExpansionModifier::ReplacePrefix { pattern, string } => match string {
                Some(string) => format!("/#{}/{}", pattern.to_string(), string.to_string()),
                None => format!("/#{}", pattern.to_string()),
            },
            ExpansionModifier::ReplaceSuffix { pattern, string } => match string {
                Some(string) => format!("/%{}/{}", pattern.to_string(), string.to_string()),
                None => format!("/%{}", pattern.to_string()),
            },
            ExpansionModifier::UpperOnce(pattern) => format!("^{}", pattern.to_string()),
            ExpansionModifier::UpperAll(pattern) => format!("^^{}", pattern.to_string()),
            ExpansionModifier::LowerOnce(pattern) => format!(",{}", pattern.to_string()),
            ExpansionModifier::LowerAll(pattern) => format!(",,{}", pattern.to_string()),
            ExpansionModifier::ErrorOnUnset(text) => format!(":?{}", text.to_string()),
            ExpansionModifier::Length => unreachable!(),
            ExpansionModifier::WhenUnset(text) => format!(":-{}", text.to_string()),
            ExpansionModifier::WhenSet(text) => format!(":+{}", text.to_string()),
            ExpansionModifier::ArrayElements => "[@]".to_string(),
            ExpansionModifier::SingleWordElements => "[*]".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GlobPattern<'a>(pub Vec<GlobPart<'a>>);

impl ToString for GlobPattern<'_> {
    fn to_string(&self) -> String {
        self.0
            .iter()
            .map(|part| part.to_string())
            .collect::<Vec<_>>()
            .join("")
    }
}

/// A element of glob patterns.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GlobPart<'a> {
    /// Matches a fixed string (`"<text>"`).
    String(Cow<'a, str>),
    /// Matches an escaped character (`"\\<char>"`).
    Escaped(char),
    /// Matches any string (`'*'`).
    AnyString,
    /// Matches any single character (`'?'`).
    AnyChar,
    /// Matches a characters range (`"[<range>]"`).
    Range(Cow<'a, str>),
}

impl ToString for GlobPart<'_> {
    fn to_string(&self) -> String {
        match self {
            GlobPart::String(text) => text.to_string(),
            GlobPart::Escaped(ch) => format!("\\{}", ch),
            GlobPart::AnyString => "*".to_string(),
            GlobPart::AnyChar => "?".to_string(),
            GlobPart::Range(range) => format!("[{}]", range),
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

impl ToString for ArrayToken<'_> {
    fn to_string(&self) -> String {
        match self {
            ArrayToken::Spacy(ch) => ch.to_string(),
            ArrayToken::Newline => '\n'.to_string(),
            ArrayToken::Comment(text) => format!("#{}", text),
            ArrayToken::Element(text) => text.to_string(),
        }
    }
}
