//! libpfu (PackFixerUpper) is a library for linting and fixing AOSC OS
//! package build script automatically.

use std::{
	fmt::Debug,
	hash::Hash,
	ops::{Deref, DerefMut},
	path::PathBuf,
};

use anyhow::Result;
use apml::ApmlFileAccess;
use async_trait::async_trait;

pub mod absets;
pub mod apml;
pub mod message;
pub mod session;
use parking_lot::RwLockUpgradableReadGuard;
pub use session::Session;

/// A checker.
#[async_trait]
pub trait Linter: 'static + Send + Sync + MetadataProvider {
	async fn apply(&self, sess: &Session) -> Result<()>;
}

/// Static non-generic metadata of a linter.
pub struct LinterMetadata {
	/// Identifier of the lint.
	pub ident: &'static str,
	/// Constructor of the underlying linter.
	pub factory: LinterFactory,
	/// Suggestions that can be produced by the linter.
	pub lints: &'static [&'static str],
}

/// Constructor of a linter;
pub type LinterFactory = &'static (dyn Send + Sync + Fn() -> Box<dyn Linter>);

impl LinterMetadata {
	/// Constructs an instance of the underlying linter.
	pub fn create(&self) -> Box<dyn Linter> {
		(self.factory)()
	}
}

impl Debug for LinterMetadata {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("LinterMetadata")
			.field("ident", &self.ident)
			.finish()
	}
}

impl PartialEq for LinterMetadata {
	fn eq(&self, other: &Self) -> bool {
		self.ident == other.ident
	}
}

impl Eq for LinterMetadata {}

impl Hash for LinterMetadata {
	fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
		self.ident.hash(state);
	}
}

pub trait MetadataProvider {
	fn metadata(&self) -> &'static LinterMetadata;
}

#[macro_export]
macro_rules! declare_linter {
    {$(#[$attr:meta])* $vis: vis $NAME: ident, $imp: ident, $lints: expr} => (
        $vis static $NAME: &$crate::LinterMetadata = &$crate::LinterMetadata {
            ident: stringify!($imp),
            factory: &|| Box::new($imp),
            lints: &$lints
        };

        $(#[$attr])* $vis struct $imp;

		impl $crate::MetadataProvider for $imp {
			fn metadata(&self) -> &'static $crate::LinterMetadata {
				return $NAME;
			}
		}
    );
}

/// Level of a lint message.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
pub enum Level {
	Note,
	Info,
	Warning,
	Error,
}

/// Static metadata of a lint.
pub struct LintMetadata {
	/// Identifier of the lint.
	pub ident: &'static str,
	/// Constructor of the underlying linter.
	pub level: Level,
	/// Default description.
	pub desc: &'static str,
}

impl Debug for LintMetadata {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("LintMetadata")
			.field("ident", &self.ident)
			.finish()
	}
}

#[macro_export]
macro_rules! declare_lint {
    ($(#[$attr:meta])* $vis: vis $NAME: ident, $id: expr, $level: ident, $desc: expr) => (
        $vis static $NAME: &$crate::LintMetadata = &$crate::LintMetadata {
            ident: $id,
            level: $crate::Level::$level,
            desc: $desc
        };
    );
}

pub fn walk_apml(sess: &'_ Session) -> Vec<ReadGuardWrapper<'_,ApmlFileAccess>> {
	let mut result = vec![ReadGuardWrapper(sess.spec.upgradable_read())];
	for subpkg in &sess.subpackages {
		for recipe in &subpkg.recipes {
			result.push(ReadGuardWrapper(recipe.defines.upgradable_read()));
		}
	}
	result
}

pub fn walk_defines(sess: &'_ Session) -> Vec<ReadGuardWrapper<'_,ApmlFileAccess>> {
	let mut result = vec![];
	for subpkg in &sess.subpackages {
		for recipe in &subpkg.recipes {
			result.push(ReadGuardWrapper(recipe.defines.upgradable_read()));
		}
	}
	result
}

pub fn walk_build_scripts(sess: &Session) -> Vec<PathBuf> {
	let mut result = vec![];
	for subpkg in &sess.subpackages {
		for recipe in &subpkg.recipes {
			for script in ["prepare", "build", "beyond"] {
				let path =
					subpkg.abbs.join(format!("{}{}", script, recipe.suffix));
				if path.is_file() {
					result.push(path);
				}
			}
		}
	}
	result
}

/// Wrapper for [RwLockUpgradableReadGuard] to make it [Send].
///
/// This struct is not a part of stable API.
pub struct ReadGuardWrapper<'a, T>(RwLockUpgradableReadGuard<'a, T>);
unsafe impl<'a, T> Send for ReadGuardWrapper<'a, T> {}

impl<'a, T> Deref for ReadGuardWrapper<'a, T> {
	type Target = RwLockUpgradableReadGuard<'a, T>;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl<'a, T> DerefMut for ReadGuardWrapper<'a, T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}

impl<T: Debug> Debug for ReadGuardWrapper<'_, T> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		Debug::fmt(&self.0, f)
	}
}
