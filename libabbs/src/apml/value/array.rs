//! String-like arrays

use std::{
    ops::{Deref, DerefMut},
    rc::Rc,
};

use crate::apml::lst;

/// A array-like string delimited with spaces.
#[derive(Debug, Clone)]
pub struct StringArray(Vec<String>);

impl StringArray {
    /// Creates a string array with given elements.
    pub fn new(values: Vec<String>) -> Self {
        Self(values)
    }

    /// Formats the string array into a LST text.
    pub fn print(&self) -> lst::Text<'static> {
        let mut words = Vec::new();
        let mut line_len = 10usize;
        let mut iter = self.0.iter();
        if let Some(value) = iter.next() {
            words.push(lst::Word::Literal(lst::LiteralPart::escape(value)));
            line_len += value.len();
        }
        for value in iter {
            if line_len + value.len() > 75 {
                // start a new line
                words.push(lst::Word::Literal(vec![
                    lst::LiteralPart::LineContinuation,
                    lst::LiteralPart::String("\t".into()),
                ]));
                words.push(lst::Word::Literal(lst::LiteralPart::escape(value)));
                line_len = 6 + value.len();
            } else {
                words.push(lst::Word::Literal(vec![lst::LiteralPart::String(
                    " ".into(),
                )]));
                words.push(lst::Word::Literal(lst::LiteralPart::escape(value)));
                line_len += value.len() + 1;
            }
        }
        lst::Text(vec![lst::TextUnit::DoubleQuote(words)])
    }
}

impl Deref for StringArray {
    type Target = Vec<String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for StringArray {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl AsRef<Vec<String>> for StringArray {
    fn as_ref(&self) -> &Vec<String> {
        &self.0
    }
}

impl AsMut<Vec<String>> for StringArray {
    fn as_mut(&mut self) -> &mut Vec<String> {
        &mut self.0
    }
}

impl<S: AsRef<str>> From<S> for StringArray {
    fn from(value: S) -> Self {
        let mut entries = Vec::new();
        let mut buffer = String::with_capacity(23);
        for char in value.as_ref().chars() {
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
        Self::new(entries)
    }
}

impl From<&StringArray> for lst::VariableValue<'_> {
    fn from(value: &StringArray) -> Self {
        Self::String(Rc::new(value.print()))
    }
}

#[cfg(test)]
mod test {
    use crate::apml::lst;

    use super::StringArray;

    #[test]
    fn test_str_array() {
        let array = StringArray::new(vec!["a".to_string(), "b".to_string()]);
        assert_eq!(array.len(), 2);
        assert_eq!(array.print().to_string(), "\"a b\"");
        assert_eq!(lst::VariableValue::from(&array).to_string(), "\"a b\"");
    }

    #[test]
    fn test_str_array_parse() {
        let array = StringArray::from("a b c\n  a   b");
        assert_eq!(array.len(), 5);
        assert_eq!(array.print().to_string(), "\"a b c a b\"");
    }

    #[test]
    fn test_str_array_format() {
        let long_str = "1234567890123456789012345678901234567890123456789012345";
        let array = StringArray::from(long_str);
        assert_eq!(array.len(), 1);
        assert_eq!(array.print().to_string(), format!("\"{long_str}\""));
        let array = StringArray::from(format!("{long_str} {long_str} 1 {long_str}"));
        assert_eq!(array.len(), 4);
        assert_eq!(array.print().to_string(), format!("\"{long_str}\\\n\t{long_str} 1\\\n\t{long_str}\""));
    }
}
