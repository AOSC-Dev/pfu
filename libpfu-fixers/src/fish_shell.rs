//! Checks for fish-shell.

use std::fs;

use anyhow::Result;
use async_trait::async_trait;
use libpfu::{
	Linter, Session, declare_lint, declare_linter,
	message::{LintMessage, Snippet},
	walk_build_scripts,
};
use log::debug;

declare_linter! {
	pub FISH_SHELL_LINTER,
	FishShellLinter,
	[
		"fish-shell-use-vendor-compl",
	]
}

declare_lint! {
	pub FISH_SHELL_USE_VENDOR_COMPL_LINT,
	"fish-shell-use-vendor-compl",
	Warning,
	"shell completions for fish should be installed to /usr/share/fish/vendor_completions.d"
}

#[async_trait]
impl Linter for FishShellLinter {
	async fn apply(&self, sess: &Session) -> Result<()> {
		if sess.package.name() == "fish" {
			debug!("skipping fish shell linter");
			return Ok(());
		}
		for path in walk_build_scripts(sess) {
			let script = fs::read_to_string(&path)?;
			if script.contains("/usr/share/fish/completions") {
				LintMessage::new(FISH_SHELL_USE_VENDOR_COMPL_LINT)
					.snippet(Snippet::new_file(&path))
					.emit(sess);
				if !sess.dry {
					let script = script.replace(
						"/usr/share/fish/completions",
						"/usr/share/fish/vendor_completions.d",
					);
					fs::write(&path, script)?;
				}
			}
		}
		Ok(())
	}
}
