//! Linter selector.

use std::collections::HashSet;

use kstring::KString;
use libpfu::LinterMetadata;
use log::{debug, warn};

use crate::linters::{self, BASELINE_LINTERS, LINTER_PRESETS, LinterPreset};

/// Selector for linters.
///
/// The selector accepts directives in the following forms:
/// - `XXXLinter`: Enable a certain linter.
/// - `XXX`: Enable a certain [linter preset][LINTER_PRESETS].
/// - `no-XXX`: Enable a certain lint or a linter preset.
/// - `no-XXXLinter`: Enable a certain linter.
pub struct LinterSelector {
	presets: HashSet<LinterPreset>,
	disabled_lints: HashSet<KString>,
	disabled_linters: HashSet<KString>,
	extra_linters: HashSet<KString>,
}

impl Default for LinterSelector {
	fn default() -> Self {
		Self {
			presets: HashSet::from([BASELINE_LINTERS]),
			disabled_lints: HashSet::new(),
			disabled_linters: HashSet::new(),
			extra_linters: HashSet::new(),
		}
	}
}

impl LinterSelector {
	/// Applies a linter selecting directive.
	pub fn apply(&mut self, directive: &str) {
		#[allow(clippy::collapsible_else_if)]
		if let Some(directive) = directive.strip_prefix("no-") {
			if directive.ends_with("Linter") {
				self.disabled_linters.insert(KString::from_ref(directive));
			} else if let Some((_, preset)) =
				LINTER_PRESETS.iter().find(|(k, _)| *k == directive)
			{
				self.presets.remove(preset);
			} else {
				self.disabled_lints.insert(KString::from_ref(directive));
			}
		} else {
			if directive.ends_with("Linter") {
				self.extra_linters.insert(KString::from_ref(directive));
			} else if let Some((_, preset)) =
				LINTER_PRESETS.iter().find(|(k, _)| *k == directive)
			{
				self.presets.insert(preset);
			} else {
				warn!("Unknown selector directive is ignored: {}", directive)
			}
		}
	}

	/// Performs the selection, returning selected linters and muted lints.
	pub fn select(
		self,
	) -> (HashSet<&'static LinterMetadata>, HashSet<KString>) {
		let mut linters = HashSet::new();
		let check = |linter: &LinterMetadata| {
			if self.disabled_linters.contains(linter.ident) {
				debug!(
					"Ignoring linter {} because it is disabled explicitly",
					linter.ident
				);
				return false;
			}
			if linter
				.lints
				.iter()
				.all(|lint| self.disabled_lints.contains(*lint))
			{
				debug!(
					"Ignoring linter {} because no lints of it is enabled",
					linter.ident
				);
				return false;
			}
			debug!("Selecting linter {}", linter.ident);
			true
		};
		for preset in self.presets {
			for linter in preset {
				if check(linter) {
					linters.insert(*linter);
				}
			}
		}
		for linter in self.extra_linters {
			if let Some(linter) = linters::find(linter.as_str()) {
				if check(linter) {
					linters.insert(linter);
				} else {
					warn!(
						"Could not select manually specified linter {}",
						linter.ident
					)
				}
			} else {
				warn!("Ignoring unknown linter {}", linter);
			}
		}
		(linters, self.disabled_lints)
	}
}
