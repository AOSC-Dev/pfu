//! ACBS Package Metadata Language (APML) syntax tree and parsers.

use std::{collections::HashMap, ops::Add};

use eval::EvalError;
use tree::ApmlParseTree;

pub mod eval;
pub mod glob;
pub mod parser;
pub mod tree;

/// A evaluated APML context.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ApmlContext {
    variables: HashMap<String, VariableValue>,
}

impl ApmlContext {
    /// Evaluates a APML source code, expanding variables.
    pub fn eval(tree: &ApmlParseTree) -> std::result::Result<Self, EvalError> {
        let mut apml = ApmlContext {
            variables: HashMap::new(),
        };
        eval::eval_parse_tree(&mut apml, tree)?;
        Ok(apml)
    }

    /// Parses a APML source code, expanding variables.
    pub fn parse(src: &str) -> std::result::Result<Self, EvalError> {
        let tree = ApmlParseTree::parse(src)?;
        Self::eval(&tree)
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

    /// Iterates over all variable names.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.variables.keys()
    }
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
    /// If the value is a string value, it will be converted into a
    /// single-element array. If the string is empty, it will be
    /// converted into a empty array.
    #[must_use]
    pub fn as_array(&self) -> Vec<String> {
        match self {
            VariableValue::String(text) => {
                if text.is_empty() {
                    vec![]
                } else {
                    vec![text.to_owned()]
                }
            }
            VariableValue::Array(els) => els.to_owned(),
        }
    }

    /// Returns the value as an array.
    ///
    /// If the value is a string value, it will be converted into a
    /// single-element array. If the string is empty, it will be
    /// converted into a empty array.
    #[must_use]
    pub fn into_array(self) -> Vec<String> {
        match self {
            VariableValue::String(text) => {
                if text.is_empty() {
                    vec![]
                } else {
                    vec![text]
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
            Self::String(val) => Self::String(format!("{}{}", val, rhs.into_string())),
            Self::Array(mut val1) => {
                val1.append(&mut rhs.into_array());
                Self::Array(val1)
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::ApmlContext;

    #[test]
    fn test_apml_parse() {
        let apml = ApmlContext::parse(
            r##"# Test APML

PKGVER=8.2
PKGDEP="x11-lib libdrm expat systemd elfutils libvdpau nettle \
        libva wayland s2tc lm-sensors libglvnd llvm-runtime libclc"
MESON_AFTER="-Ddri-drivers-path=/usr/lib/xorg/modules/dri \
             -Db_ndebug=true" 
MESON_AFTER__AMD64=" \
             ${MESON_AFTER} \
             -Dlibunwind=true"
A="${b[@]}"
"##,
        )
        .unwrap();
        dbg!(&apml);
    }
}
