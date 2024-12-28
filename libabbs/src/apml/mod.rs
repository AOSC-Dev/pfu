//! ACBS Package Metadata Language (APML) parsers.

use std::{collections::HashMap, ops::Range};

pub mod ast;
pub mod parser;

/// A parsed APML file.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Apml {
    variables: HashMap<String, Variable>,
}

/// A variable declared in APML.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Variable {
    /// The name of the variable.
    pub name: String,
    /// The value of the variable.
    pub value: VariableValue,
    /// The raw code of the variable value.
    pub raw: String,
    /// The code location of the variable.
    pub location: Range<usize>,
}

/// Value of variables.
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum VariableValue {
    String(String),
    Array(Vec<String>),
}

impl Apml {
    /// Parses a APML source code, expanding variables.
    pub fn parse(source: &str) -> std::result::Result<Self, ()> {
        let mut apml = Apml {
            variables: HashMap::new(),
        };
        // ast::parse_source(&mut apml, source)?;
        Ok(apml)
    }
}

#[cfg(test)]
mod test {
    use super::Apml;

    #[test]
    fn test_apml_parse() {
        let apml = Apml::parse(
            r##"# Test APML

PKGVER=8.2
PKGDEP="x11-lib libdrm expat systemd elfutils libvdpau nettle \
        libva wayland s2tc lm-sensors libglvnd llvm-runtime libclc"
MESON_AFTER="-Ddri-drivers-path=/usr/lib/xorg/modules/dri \
             -Db_ndebug=true" 
MESON_AFTER__AMD64=" \
             ${MESON_AFTER} \
             -Dlibunwind=true"
A="${b[0]}"
"##,
        )
        .unwrap();
    }
}
