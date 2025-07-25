//! ABBS tree operators.

use std::{
	fmt::{Debug, Display},
	fs,
	ops::Deref,
	path::{Path, PathBuf},
};

use kstring::KString;
use thiserror::Error;

use crate::apml::{ApmlContext, ApmlError};

#[derive(Debug, Clone)]
pub struct AbbsTree(PathBuf);

impl AbbsTree {
	/// Opens a ABBS tree at the given path.
	pub fn new<P: AsRef<Path>>(path: P) -> Self {
		Self(path.as_ref().to_owned())
	}

	/// Returns the path of tree.
	pub fn as_path(&self) -> &Path {
		&self.0
	}

	/// Returns the path of tree.
	pub fn into_path(self) -> PathBuf {
		self.0
	}

	/// Creates an owned [`PathBuf`] with `path` adjoined to `self`.
	#[must_use]
	pub fn join<P: AsRef<Path>>(&self, path: P) -> PathBuf {
		self.as_path().join(path.as_ref())
	}
}

impl AsRef<Path> for AbbsTree {
	fn as_ref(&self) -> &Path {
		self.as_path()
	}
}

impl AbbsTree {
	/// Iterates over sections in the tree, e.g. `app-admin` and `runtime-display`.
	pub fn sections(&self) -> AbbsResult<Vec<SectionName>> {
		let mut result = Vec::new();
		for entry in self.as_path().read_dir()? {
			let entry = entry?;
			if entry.file_type()?.is_dir()
				&& let Some(name) = entry.file_name().to_str()
				&& !name.starts_with('.')
				&& name.contains('-')
			{
				result.push(SectionName::from_ref(name));
			}
		}
		Ok(result)
	}

	/// Creates a source package accessor.
	pub fn package<S: AsRef<str>>(
		&self,
		section: &SectionName,
		name: S,
	) -> Option<AbbsSourcePackage> {
		let path = self.as_path().join(section.as_str()).join(name.as_ref());
		if path.is_dir() && path.join("spec").exists() {
			Some(AbbsSourcePackage(path))
		} else {
			None
		}
	}

	/// Iterates over all source packages and creates accessors.
	pub fn all_packages(&self) -> AbbsResult<Vec<AbbsSourcePackage>> {
		let mut result = Vec::new();
		for section in self.sections()? {
			result.append(&mut self.section_packages(&section)?);
		}
		Ok(result)
	}

	/// Iterates over all source packages in a certain section and creates accessors.
	pub fn section_packages(
		&self,
		section: &SectionName,
	) -> AbbsResult<Vec<AbbsSourcePackage>> {
		let mut result = Vec::new();
		for entry in self.join(section.as_str()).read_dir()? {
			let entry = entry?;
			if entry.file_type()?.is_dir() {
				result.push(AbbsSourcePackage::new(entry.path()));
			}
		}
		Ok(result)
	}

	/// Finds a source package and creates an accessor for it.
	pub fn find_package<S: AsRef<str>>(
		&self,
		name: S,
	) -> AbbsResult<AbbsSourcePackage> {
		self.sections()?
			.iter()
			.find_map(|section| self.package(section, name.as_ref()))
			.ok_or_else(|| {
				AbbsError::PackageNotFound(name.as_ref().to_string())
			})
	}

	/// Returns a subpackage with the given name.
	///
	/// This will first try to find the package in the source package
	/// with the same name. If failed to do so, it will fallback to
	/// search the whole tree.
	///
	/// Thus this is a very high-cost operation and you must use it
	/// carefully.
	pub fn find_subpackage<S: AsRef<str>>(
		&self,
		name: S,
	) -> AbbsResult<AbbsSubPackage> {
		let name = name.as_ref();
		match self.find_package(name) {
			Ok(package) => {
				for subpkg in package.subpackages()? {
					if let Ok(subpkg_name) = subpkg.name()
						&& subpkg_name == name
					{
						return Ok(subpkg);
					}
				}
			}
			Err(AbbsError::PackageNotFound(_)) => {}
			Err(err) => return Err(err),
		}
		for package in self.all_packages()? {
			for subpkg in package.subpackages()? {
				if let Ok(subpkg_name) = subpkg.name()
					&& subpkg_name == name
				{
					return Ok(subpkg);
				}
			}
		}
		Err(AbbsError::PackageNotFound(name.to_string()))
	}
}

#[derive(Debug, Error)]
pub enum AbbsError {
	#[error("I/O Error: {0}")]
	IoError(#[from] std::io::Error),
	#[error("Package not found: {0}")]
	PackageNotFound(String),
	#[error(transparent)]
	ApmlError(#[from] ApmlError),
}

pub type AbbsResult<T> = Result<T, AbbsError>;

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

#[derive(Clone)]
pub struct AbbsSourcePackage(PathBuf);

impl AbbsSourcePackage {
	/// Opens a ABBS source package at the given path.
	pub fn new<P: AsRef<Path>>(path: P) -> Self {
		Self(path.as_ref().to_owned())
	}

	/// Returns the path of package.
	pub fn as_path(&self) -> &Path {
		&self.0
	}

	/// Returns the path of package.
	pub fn into_path(self) -> PathBuf {
		self.0
	}

	/// Creates an owned [`PathBuf`] with `path` adjoined to `self`.
	#[must_use]
	pub fn join<P: AsRef<Path>>(&self, path: P) -> PathBuf {
		self.as_path().join(path.as_ref())
	}

	/// Returns the name of package.
	pub fn name(&self) -> &str {
		self.0
			.file_name()
			.expect("ABBS source package cannot be root directory")
			.to_str()
			.expect("ABBS source package name must be ASCII string")
	}

	/// Returns the section of package.
	pub fn section(&self) -> SectionName {
		SectionName::from_ref(
			self.0
				.parent()
				.expect("ABBS source package cannot be root directory")
				.file_name()
				.expect("ABBS source package cannot be root directory")
				.to_str()
				.expect("ABBS source package name must be ASCII string"),
		)
	}

	/// Returns the tree containing the package.
	pub fn tree(&self) -> AbbsTree {
		AbbsTree::new(
			self.0
				.parent()
				.expect("ABBS source package cannot be root directory")
				.parent()
				.expect("ABBS source package cannot be root directory"),
		)
	}

	/// Returns a list of subpackages of this package.
	pub fn subpackages(&self) -> AbbsResult<Vec<AbbsSubPackage>> {
		let mut result = Vec::new();
		for entry in self.as_path().read_dir()? {
			let entry = entry?;
			if entry.file_type()?.is_dir() {
				result.push(AbbsSubPackage::new(entry.path()));
			}
		}
		Ok(result)
	}

	/// Returns a certain subpackage of this package.
	pub fn subpackage<S: AsRef<str>>(&self, dir: S) -> Option<AbbsSubPackage> {
		let path = self.as_path().join(dir.as_ref());
		if path.is_dir() && path.join("defines").is_file() {
			Some(AbbsSubPackage::new(path))
		} else {
			None
		}
	}
}

impl AsRef<Path> for AbbsSourcePackage {
	fn as_ref(&self) -> &Path {
		self.as_path()
	}
}

impl PartialEq for AbbsSourcePackage {
	fn eq(&self, other: &Self) -> bool {
		self.name() == other.name()
	}
}

impl Eq for AbbsSourcePackage {}

impl PartialOrd for AbbsSourcePackage {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		if self.tree().as_path() == other.tree().as_path() {
			self.name().partial_cmp(other.name())
		} else {
			None
		}
	}
}

impl Debug for AbbsSourcePackage {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_fmt(format_args!("{}/{}", self.section(), self.name()))
	}
}

#[derive(Debug, Clone)]
pub struct AbbsSubPackage(PathBuf);

impl AbbsSubPackage {
	/// Opens a ABBS sub-package at the given path.
	pub fn new<P: AsRef<Path>>(path: P) -> Self {
		Self(path.as_ref().to_owned())
	}

	/// Returns the path of package.
	pub fn as_path(&self) -> &Path {
		&self.0
	}

	/// Returns the path of package.
	pub fn into_path(self) -> PathBuf {
		self.0
	}

	/// Creates an owned [`PathBuf`] with `path` adjoined to `self`.
	#[must_use]
	pub fn join<P: AsRef<Path>>(&self, path: P) -> PathBuf {
		self.as_path().join(path.as_ref())
	}

	/// Returns the directory name of package.
	pub fn dir_name(&self) -> &str {
		self.0
			.file_name()
			.expect("ABBS source package cannot be root directory")
			.to_str()
			.expect("ABBS source package name must be ASCII string")
	}

	/// Returns the package name of sub-package.
	///
	/// Note: getting the name requires evaluating defines file
	/// and is a high-cost operation. The caller should cache the name
	/// as much as possible.
	pub fn name(&self) -> AbbsResult<String> {
		Ok(ApmlContext::eval_source(&fs::read_to_string(
			self.join("defines"),
		)?)?
		.read("PKGNAME")
		.into_string())
	}

	/// Returns the source package.
	pub fn source_package(&self) -> AbbsSourcePackage {
		AbbsSourcePackage::new(
			self.0
				.parent()
				.expect("ABBS source package cannot be root directory"),
		)
	}

	/// Returns all modifiers suffixes.
	///
	/// For example, `""` for no modifier variant, `".stage2"` for stage2 variant.
	pub fn modifier_suffixes(&self) -> AbbsResult<Vec<KString>> {
		Ok(self
			.as_path()
			.read_dir()?
			.collect::<Result<Vec<_>, _>>()?
			.into_iter()
			.filter(|entry| {
				entry.file_type().map(|ty| ty.is_file()).unwrap_or(false)
			})
			.filter_map(|entry| {
				let name = entry.file_name();
				let name = name.to_str().unwrap_or_default();
				name.strip_prefix("defines").map(KString::from_ref)
			})
			.collect::<Vec<_>>())
	}
}

impl AsRef<Path> for AbbsSubPackage {
	fn as_ref(&self) -> &Path {
		self.as_path()
	}
}

impl PartialEq for AbbsSubPackage {
	fn eq(&self, other: &Self) -> bool {
		self.0 == other.0
	}
}

impl Eq for AbbsSubPackage {}

impl PartialOrd for AbbsSubPackage {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		if self.source_package().tree().as_path()
			== other.source_package().tree().as_path()
		{
			(self.source_package(), self.dir_name())
				.partial_cmp(&(self.source_package(), other.dir_name()))
		} else {
			None
		}
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_tree() {
		let tree = AbbsTree::new("testrepo");
		assert_eq!(tree.sections().unwrap(), vec!["app-admin".into()]);
		assert!(tree.package(&"app-admin".into(), "not-a-package").is_none());
		assert!(
			tree.package(&"app-admin".into(), "not-a-package2")
				.is_none()
		);
		assert!(tree.package(&"app-admin".into(), "test1").is_some());
		assert_eq!(
			tree.section_packages(&"app-admin".into()).unwrap().len(),
			3
		);
		assert_eq!(tree.all_packages().unwrap().len(), 3);
		assert_eq!(tree.find_package("test1").unwrap().name(), "test1");
		assert_eq!(
			tree.find_subpackage("test1").unwrap().name().unwrap(),
			"test1"
		);
		assert_eq!(
			tree.find_subpackage("test2-host").unwrap().name().unwrap(),
			"test2-host"
		);
	}

	#[test]
	fn test_section_name() {
		let sec = SectionName::from_ref("app");
		assert_eq!(sec.as_str(), "app");
		let sec = SectionName::from_string("app".to_string());
		assert_eq!(sec.as_str(), "app");
		assert!(!sec.deref().contains('-'));
		assert_eq!(format!("{sec}"), "app");
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

	#[test]
	fn test_source_package() {
		let tree = AbbsTree::new("testrepo");
		let pkg = tree.package(&"app-admin".into(), "test1").unwrap();
		assert_eq!(pkg.name(), "test1");
		assert_eq!(pkg.section().as_str(), "app-admin");
		assert_eq!(pkg.tree().sections().unwrap(), vec!["app-admin".into()]);
		assert_eq!(pkg.subpackages().unwrap().len(), 1);
		assert_eq!(pkg.subpackages().unwrap()[0].dir_name(), "autobuild");
		assert_eq!(
			pkg.subpackages().unwrap()[0].source_package().name(),
			"test1"
		);
		assert!(pkg.subpackage("autobuild").is_some());
		assert!(pkg.subpackage("01-host").is_none());
	}

	#[test]
	fn test_subpackage() {
		let tree = AbbsTree::new("testrepo");
		let pkg = tree.package(&"app-admin".into(), "test2").unwrap();
		assert_eq!(pkg.subpackages().unwrap().len(), 2);
		let host = pkg.subpackage("01-host").unwrap();
		let guest = pkg.subpackage("02-guest").unwrap();
		assert_eq!(host.dir_name(), "01-host");
		assert_eq!(host.name().unwrap(), "test2-host");
		assert_eq!(guest.dir_name(), "02-guest");
		assert_eq!(guest.name().unwrap(), "test2-guest");
		assert_eq!(host.modifier_suffixes().unwrap().len(), 2);
		assert!(
			host.modifier_suffixes()
				.unwrap()
				.contains(&KString::from_static(".stage2"))
		);
		assert!(
			host.modifier_suffixes()
				.unwrap()
				.contains(&KString::from_static(""))
		);
		assert_eq!(guest.modifier_suffixes().unwrap().len(), 1);
	}
}
