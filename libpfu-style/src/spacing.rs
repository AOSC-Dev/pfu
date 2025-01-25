//! Spaces and newline checks.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use libpfu::{
	Linter, Session, declare_lint, declare_linter,
	message::{LintMessage, Snippet},
};

declare_linter! {
	pub EXTRA_SPACES_LINTER,
	ExtraSpacesLinter,
	["extra-spaces"]
}

declare_lint! {
	pub EXTRA_SPACES_LINT,
	"extra-spaces",
	Warning,
	"extra spaces should be removed"
}

#[async_trait]
impl Linter for ExtraSpacesLinter {
	async fn apply(&self, sess: &mut Session) -> Result<()> {
		Ok(())
	}
}
