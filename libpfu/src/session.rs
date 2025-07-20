//! Context
//!
//! To apply a lint or fix to a package, callers must prepare a [Context],
//! providing enough information to fixers.

use std::sync::{Arc, OnceLock};

use anyhow::{Result, bail};
use futures::executor::block_on;
use kstring::KString;
use libabbs::tree::{AbbsSourcePackage, AbbsSubPackage, AbbsTree};
use log::debug;
use parking_lot::{Mutex, RwLock};

use crate::{
	absets::Autobuild4Data, apml::ApmlFileAccess, message::LintMessage,
};

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
	/// Autobuild4 data.
	pub ab4_data: Option<Arc<Autobuild4Data>>,

	/// Lazily initialized source FS
	source_storage: tokio::sync::RwLock<Option<Arc<opendal::Operator>>>,
	/// Lazily initialized HTTP client
	http_client: OnceLock<reqwest::Client>,
	/// Receiver for lint messages.
	pub(crate) outbox: Mutex<Vec<LintMessage>>,
}

impl Session {
	pub fn new(
		tree: AbbsTree,
		package: AbbsSourcePackage,
		ab4_data: Option<Arc<Autobuild4Data>>,
	) -> Result<Self> {
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
			ab4_data,
			source_storage: tokio::sync::RwLock::default(),
			http_client: OnceLock::default(),
			outbox: Mutex::new(Vec::new()),
		})
	}

	#[allow(clippy::await_holding_lock)]
	pub async fn source_fs(&self) -> Result<Arc<opendal::Operator>> {
		if self.offline {
			bail!("offline mode")
		} else if let Some(result) = self.source_storage.read().await.as_ref() {
			Ok(result.clone())
		} else {
			let mut write = self.source_storage.write().await;
			if let Some(result) = write.as_ref() {
				Ok(result.clone())
			} else {
				*write = Some(Arc::new(
					libpfu_source::open(block_on(async {
						self.spec.write().ctx().cloned()
					})?)
					.await?,
				));
				Ok(write.as_ref().unwrap().clone())
			}
		}
	}

	pub fn take_messages(&self) -> Vec<LintMessage> {
		let mut result = Vec::new();
		result.append(&mut *self.outbox.lock());
		result
	}

	pub fn http_client(&self) -> Result<reqwest::Client> {
		if self.offline {
			bail!("offline mode")
		}
		// TODO: use OnceLock::get_or_try_init after its stablization
		let client = self.http_client.get_or_init(|| {
			reqwest::ClientBuilder::new()
				.connect_timeout(std::time::Duration::from_secs(10))
				.read_timeout(std::time::Duration::from_secs(10))
				.user_agent(format!(
					"libpfu/{} (https://github.com/AOSC-Dev/pfu)",
					env!("CARGO_PKG_VERSION")
				))
				.build()
				.expect("HTTP client initialization failed")
		});
		Ok(client.clone())
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
			ApmlFileAccess::open(abbs.join(format!("defines{suffix}")))?;
		Ok(Self {
			suffix,
			defines: RwLock::new(defines),
		})
	}
}
