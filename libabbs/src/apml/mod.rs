//! ACBS Package Metadata Language (APML) parsers.

use std::collections::HashMap;

/// A parsed APML file.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Apml {
    variables: HashMap<String, Variable>,
}

/// A variable declared in APML.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Variable {
    pub name: String,
    pub value: String,
    pub raw: String,
}

impl Apml {
    /// Parses a APML file, expanding variables.
    pub fn parse() {}
}
