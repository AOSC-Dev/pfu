//! Context
//!
//! To apply a lint or fix to a package, callers must prepare a [Context],
//! providing enough information to fixers.

use std::sync::Arc;

use anyhow::Result;
use kstring::KString;
use libabbs::tree::{AbbsSourcePackage, AbbsSubPackage, AbbsTree};
use log::debug;
use parking_lot::{Mutex, RwLock};

use crate::{apml::ApmlFileAccess, message::LintMessage, source};

/// A context including information related to the package to fix.
pub struct Session {
	/// ABBS tree accessor.
	pub tree: AbbsTree,
	/// Package accessor.
	pub package: AbbsSourcePackage,
	/// Dry-run switch.
	pub dry: bool,
	/// Offline mode switch.
	pub offline: bool,
	/// Spec file.
	pub spec: RwLock<ApmlFileAccess>,
	/// Sub-packages
	pub subpackages: Vec<SubpackageSession>,

	/// Lazily initialized source
	source_storage: tokio::sync::RwLock<Option<Arc<opendal::Operator>>>,
	/// Receiver for lint messages.
	pub(crate) outbox: Mutex<Vec<LintMessage>>,
}

impl Session {
	pub fn new(tree: AbbsTree, package: AbbsSourcePackage) -> Result<Self> {
		let spec = ApmlFileAccess::open(package.join("spec"))?;
		let mut subpackages = Vec::new();
		for subpackage in package.subpackages()? {
			subpackages.push(SubpackageSession::new(subpackage)?);
		}
		debug!(
			"Loaded {} sub-packages for {}",
			subpackages.len(),
			package.name()
		);

		Ok(Self {
			tree,
			package,
			dry: false,
			offline: false,
			spec: RwLock::new(spec),
			subpackages,
			source_storage: tokio::sync::RwLock::default(),
			outbox: Mutex::new(Vec::new()),
		})
	}

	#[allow(clippy::await_holding_lock)]
	pub async fn source_fs(&self) -> Result<Arc<opendal::Operator>> {
		if let Some(result) = self.source_storage.read().await.as_ref() {
			Ok(result.clone())
		} else {
			let mut write = self.source_storage.write().await;
			if let Some(result) = write.as_ref() {
				Ok(result.clone())
			} else {
				*write = Some(source::open(self).await?.into());
				Ok(write.as_ref().unwrap().clone())
			}
		}
	}

	pub fn take_messages(&self) -> Vec<LintMessage> {
		let mut result = Vec::new();
		result.append(&mut *self.outbox.lock());
		result
	}
}

/// A context for a certain sub-package.
pub struct SubpackageSession {
	/// ABBS sub-package accessor
	pub abbs: AbbsSubPackage,
	/// Recipes.
	pub recipes: Vec<RecipeSession>,
}

impl SubpackageSession {
	pub fn new(abbs: AbbsSubPackage) -> Result<Self> {
		let mut recipes = Vec::new();
		for suffix in abbs.modifier_suffixes()? {
			recipes.push(RecipeSession::new(&abbs, suffix)?);
		}
		debug!(
			"Loaded {} recipes for {}/{}",
			recipes.len(),
			abbs.source_package().name(),
			abbs.dir_name()
		);
		Ok(Self { abbs, recipes })
	}
}

/// A context for a certain recipe.
pub struct RecipeSession {
	/// ABBS sub-package accessor
	pub suffix: KString,
	/// Defines
	pub defines: RwLock<ApmlFileAccess>,
}

impl RecipeSession {
	pub fn new(abbs: &AbbsSubPackage, suffix: KString) -> Result<Self> {
		let defines =
			ApmlFileAccess::open(abbs.join(format!("defines{}", suffix)))?;
		Ok(Self {
			suffix,
			defines: RwLock::new(defines),
		})
	}
}
