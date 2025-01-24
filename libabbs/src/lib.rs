//! libabbs is a utilities library for AOSC OS packaging scripts maintenance
//! tasks.

use std::{borrow::Borrow, fmt::Display, ops::Deref};

use kstring::KString;

#[cfg(feature = "apml")]
pub mod apml;
#[cfg(feature = "tree")]
pub mod tree;

/// Name of a package section or a category, e.g. `app-admin` and `app-devel`.
///
/// The first part is called "category" and the latter is called "section".
/// The category part is required but the section part is optional.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
#[repr(transparent)]
pub struct SectionName(KString);

impl SectionName {
    /// Wraps a `aaa-bbb`-like or `aaa`-like string as a section name.
    #[must_use]
    pub fn from_ref(string: &str) -> Self {
        Self(KString::from_ref(string))
    }

    /// Wraps a `aaa-bbb`-like or `aaa`-like [String] as a section name.
    #[must_use]
    pub fn from_string(string: String) -> Self {
        Self(KString::from_string(string))
    }

    /// Wraps a `aaa-bbb`-like or `aaa`-like static string as a section name.
    #[must_use]
    pub fn from_static(string: &'static str) -> Self {
        Self(KString::from_static(string))
    }
}

impl<S: Into<KString>> From<S> for SectionName {
    fn from(value: S) -> Self {
        Self(value.into())
    }
}

impl Display for SectionName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl SectionName {
    /// Returns the full section name, including both two parts.
    #[must_use]
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Gets the category part of the section, e.g. `app` and `runtime`.
    #[must_use]
    pub fn category(&self) -> &str {
        self.0
            .split_once('-')
            .map(|(cat, _)| cat)
            .unwrap_or(&self.0)
    }

    /// Gets the section part of the section, e.g. `devel` and `admin`.
    #[must_use]
    pub fn section(&self) -> Option<&str> {
        self.0.split_once('-').map(|(_, sec)| sec)
    }
}

impl AsRef<str> for SectionName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Deref for SectionName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_section_name() {
        let sec = SectionName::from_ref("app");
        assert_eq!(sec.as_str(), "app");
        let sec = SectionName::from_string("app".to_string());
        assert_eq!(sec.as_str(), "app");
        assert_eq!(sec.deref().contains('-'), false);
        assert_eq!(format!("{}", sec), "app");
        let sec = SectionName::from_static("app");
        assert_eq!(sec.as_str(), "app");
        assert_eq!(sec.as_ref(), "app");
        assert_eq!(sec.category(), "app");
        assert_eq!(sec.section(), None);
        let sec = SectionName::from("app");
        assert_eq!(sec.as_str(), "app");
        let sec = SectionName::from_static("app-devel");
        assert_eq!(sec.as_str(), "app-devel");
        assert_eq!(sec.as_ref(), "app-devel");
        assert_eq!(sec.category(), "app");
        assert_eq!(sec.section(), Some("devel"));
    }
}
