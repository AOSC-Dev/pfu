//! `CHKUPDATE` checks.

use anyhow::Result;
use async_trait::async_trait;
use libabbs::apml::value::union::Union;
use libpfu::{
	Linter, Session, declare_lint, declare_linter,
	message::{LintMessage, Snippet},
	walk_apml,
};
use log::debug;

declare_linter! {
	pub CHKUPDATE_LINTER,
	ChkUpdateLinter,
	[
		"unknown-findupdate-tag",
		"prefer-anitya",
	]
}

declare_lint! {
	pub UNKNOWN_FINDUPDATE_TAG_LINT,
	"unknown-findupdate-tag",
	Error,
	"unknown handler found in CHKUPDATE"
}

declare_lint! {
	pub PREFER_ANITYA_LINT,
	"prefer-anitya",
	Warning,
	"prefer to use Anitya for version checking"
}

#[async_trait]
impl Linter for ChkUpdateLinter {
	async fn apply(&self, sess: &Session) -> Result<()> {
		for mut apml in walk_apml(sess) {
			debug!("Checking CHKUPDATE in {:?}", apml);
			let (chkupdate, chkupdate_idx) = apml.with_upgraded(|apml| {
				(
					apml.ctx().map(|ctx| ctx.read("CHKUPDATE").into_string()),
					apml.read_with_editor(|editor| {
						editor
							.find_var("CHKUPDATE")
							.unzip()
							.0
							.unwrap_or_default()
					}),
				)
			});
			let chkupdate = chkupdate?;
			let chkupdate = chkupdate.trim();
			if chkupdate.is_empty() {
				debug!("CHKUPDATE is not defined");
				return Ok(());
			}

			let un = Union::try_from(chkupdate)?;
			match un.tag.to_ascii_lowercase().as_str() {
				"anitya" => {}
				"github" | "gitweb" | "git" | "html" | "gitlab" => {
					LintMessage::new(PREFER_ANITYA_LINT)
						.note(format!(
							"CHKUPDATE with tag {} should be converted into anitya",
							un.tag
						))
						.snippet(Snippet::new_index(sess, &apml, chkupdate_idx))
						.emit(sess);
				}
				_ => {
					LintMessage::new(UNKNOWN_FINDUPDATE_TAG_LINT)
						.note(format!(
							"CHKUPDATE with tag {} is unsupported",
							un.tag
						))
						.snippet(Snippet::new_index(sess, &apml, chkupdate_idx))
						.emit(sess);
				}
			}
		}
		Ok(())
	}
}
