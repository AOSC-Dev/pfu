//! libpfu (PackFixerUpper) is a library for linting and fixing AOSC OS
//! package build script automatically.

use std::{fmt::Debug, hash::Hash};

use anyhow::Result;
use apml::ApmlFileAccess;
use async_trait::async_trait;

pub mod apml;
pub mod message;
pub mod session;
use parking_lot::RwLockUpgradableReadGuard;
pub use session::Session;

/// A checker.
#[async_trait]
pub trait Linter: 'static + Send + Sync {
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
pub type LinterFactory = &'static (dyn Send + Sync + (Fn() -> Box<dyn Linter>));

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

#[macro_export]
macro_rules! declare_linter {
    {$(#[$attr:meta])* $vis: vis $NAME: ident, $imp: ident, $lints: expr} => (
        $vis static $NAME: &$crate::LinterMetadata = &$crate::LinterMetadata {
            ident: stringify!($imp),
            factory: &|| Box::new($imp),
            lints: &$lints
        };

        $(#[$attr])* $vis struct $imp;
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

pub fn walk_apml(
	sess: &Session,
) -> Vec<RwLockUpgradableReadGuard<ApmlFileAccess>> {
	let mut result = vec![sess.spec.upgradable_read()];
	for subpkg in &sess.subpackages {
		for recipe in &subpkg.recipes {
			result.push(recipe.defines.upgradable_read());
		}
	}
	result
}

pub fn walk_defines(
	sess: &Session,
) -> Vec<RwLockUpgradableReadGuard<ApmlFileAccess>> {
	let mut result = vec![];
	for subpkg in &sess.subpackages {
		for recipe in &subpkg.recipes {
			result.push(recipe.defines.upgradable_read());
		}
	}
	result
}
