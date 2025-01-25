//! `SRCS` checks.

use anyhow::Result;
use async_trait::async_trait;
use lazy_static::lazy_static;
use libabbs::apml::value::{array::StringArray, union::Union};
use libpfu::{
	Linter, Session, declare_lint, declare_linter,
	message::{LintMessage, Snippet},
	walk_apml,
};
use log::debug;
use regex::{Regex, Replacer};

declare_linter! {
	pub SRCS_LINTER,
	SrcsLinter,
	[
		"unknown-fetch-tag",
		"prefer-handler-srcs",
	]
}

declare_lint! {
	pub UNKNOWN_SRC_HANDLER_LINT,
	"unknown-handler-in-srcs",
	Error,
	"unknown handler found in SRCS"
}

declare_lint! {
	pub PREFER_SPECIFIC_SRC_HANDLER_LINT,
	"prefer-specific-src-handler",
	Warning,
	"use more-specific handler for SRCS"
}

lazy_static! {
	pub static ref REGEX_PYPI: Regex = Regex::new(r##"https://pypi\.io/packages/source/[A-Za-z]/(?P<name>[A-Za-z0-9\._\-]+)/([A-Za-z0-9\._\-]+)"##).unwrap();
	pub static ref REGEX_PYPI_FULL: Regex = Regex::new(r##"(tarball|tbl)::https://pypi\.io/packages/source/[A-Za-z]/(?P<name>[A-Za-z0-9\._\-]+)/([A-Za-z0-9\._\-]+)-(?P<version>\$VER|\$\{[^}]+\})\.tar(\.gz|\.xz|\.bz2|\.bz|\.zst|)"##).unwrap();
	pub static ref REGEX_GH_TAR: Regex = Regex::new(r##"https:\/\/github\.com\/([a-zA-Z_-]+)\/([a-zA-Z_-]+)\/archive\/"##).unwrap();
	pub static ref REGEX_GH_RELEASE: Regex = Regex::new(r##"https:\/\/github\.com\/([a-zA-Z_-]+)\/([a-zA-Z_-]+)\/releases\/download\/"##).unwrap();
}

#[async_trait]
impl Linter for SrcsLinter {
	async fn apply(&self, sess: &Session) -> Result<()> {
		for mut apml in walk_apml(sess) {
			debug!("Looking for less-specific handlers in SRCS in {:?}", apml);
			let (srcs, srcs_idx) = apml.with_upgraded(|apml| {
				(
					apml.ctx().map(|ctx| ctx.read("SRCS").into_string()),
					apml.read_with_editor(|editor| {
						editor.find_var("SRCS").unzip().0.unwrap_or_default()
					}),
				)
			});
			let mut srcs = srcs?;
			if srcs.starts_with("https://") {
				srcs = format!("tbl::{}", srcs);
			}
			let mut srcs = StringArray::from(srcs);

			let mut dirty = false;
			for (idx, mut src) in srcs.iter_mut().enumerate() {
				let un = Union::try_from(src.as_str())?;
				match un.tag.to_ascii_lowercase().as_str() {
					"tarball" | "tbl" => {
						if let Some(arg) = un.argument {
							if let Some(cap) = REGEX_PYPI.captures(&arg) {
								LintMessage::new(
									PREFER_SPECIFIC_SRC_HANDLER_LINT,
								)
								.note(format!(
									"source {} should be replaced with pypi::{}",
									idx, &cap["name"],
								))
								.snippet(Snippet::new(sess, &apml, srcs_idx))
								.emit(sess);
								if !sess.dry {
									apml.with_upgraded(|apml| {
										apml.with_text(|text| {
											REGEX_PYPI_FULL
												.replace(
													&text,
													"pypi::version=${version}::${name}",
												)
												.to_string()
										})
									})?;
								}
							} else if REGEX_GH_TAR.is_match(&arg) {
								LintMessage::new(
									PREFER_SPECIFIC_SRC_HANDLER_LINT,
								)
								.note(format!(
									"source {} should be replaced with git::",
									idx
								))
								.snippet(Snippet::new(sess, &apml, srcs_idx))
								.emit(sess);
							} else if REGEX_GH_RELEASE.is_match(&arg) {
								LintMessage::new(
									PREFER_SPECIFIC_SRC_HANDLER_LINT,
								)
								.note(format!(
									"source {} should be replaced with git::",
									idx
								))
								.snippet(Snippet::new(sess, &apml, srcs_idx))
								.emit(sess);
							}
						}
					}
					"git" | "svn" | "bzr" | "hg" | "fossil" | "file"
					| "pypi" | "none" => {}
					_ => {
						LintMessage::new(UNKNOWN_SRC_HANDLER_LINT)
							.note(format!(
								"source {} with tag {} is unsupported",
								idx, un.tag
							))
							.snippet(Snippet::new(sess, &apml, srcs_idx))
							.emit(sess);
					}
				}
			}
		}
		Ok(())
	}
}
