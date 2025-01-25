//! `SRCS` checks.

use std::sync::LazyLock;

use anyhow::Result;
use async_trait::async_trait;
use libabbs::apml::value::{array::StringArray, union::Union};
use libpfu::{
	Linter, Session, declare_lint, declare_linter,
	message::{LintMessage, Snippet},
	walk_apml,
};
use log::debug;
use regex::Regex;

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

const REGEX_TBL: &str = "(tarball|tbl)::";
const REGEX_VERSION_TAR: &str = r##"(?P<version>\$VER|[a-zA-Z0-9\.]*\$\{[^}]+\}|[^\.]+)\.tar(\.gz|\.xz|\.bz2|\.bz|\.zstd|\.zst|)"##;

static REGEX_PYPI: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r##"https://pypi\.io/packages/source/[A-Za-z]/(?P<name>[A-Za-z0-9\._\-]+)/([A-Za-z0-9\._\-]+)"##).unwrap()
});
static REGEX_PYPI_FULL: LazyLock<Regex> = LazyLock::new(|| {
	let regex = format!(
		"{}{}{}",
		REGEX_TBL,
		r##"https://pypi\.io/packages/source/[A-Za-z]/(?P<name>[A-Za-z0-9\._\-]+)/([A-Za-z0-9\._\-]+)-"##,
		REGEX_VERSION_TAR
	);
	Regex::new(&regex).unwrap()
});
static REGEX_GH_TAR: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(
		r##"https://github\.com/(?<user>[a-zA-Z_-]+)/(?<repo>[a-zA-Z_-]+)/archive/"##,
	)
	.unwrap()
});
static REGEX_GH_TAR_FULL: LazyLock<Regex> = LazyLock::new(|| {
	let regex = format!(
		"{}{}{}",
		REGEX_TBL,
		r##"https://github\.com/(?<user>[a-zA-Z_-]+)/(?<repo>[a-zA-Z_-]+)/archive/"##,
		REGEX_VERSION_TAR
	);
	Regex::new(&regex).unwrap()
});

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
			let mut srcs = StringArray::from(srcs?);

			for (idx, src) in srcs.iter_mut().enumerate() {
				let un = if src.starts_with("https://") {
					Union::try_from(format!("tbl::{}", src).as_str())?
				} else {
					Union::try_from(src.as_str())?
				};

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
							} else if let Some(cap) =
								REGEX_GH_TAR.captures(&arg)
							{
								LintMessage::new(
									PREFER_SPECIFIC_SRC_HANDLER_LINT,
								)
								.note(format!(
									"source {} should be replaced with git::https://github.com/{}/{}.git",
									idx, &cap["user"], &cap["repo"],
								))
								.snippet(Snippet::new(sess, &apml, srcs_idx))
								.emit(sess);
								if !sess.dry {
									apml.with_upgraded(|apml| {
										apml.with_text(|text| {
											REGEX_GH_TAR_FULL
												.replace(
													&text,
													"git::commit=tags/${version}::https://github.com/${user}/${repo}.git",
												)
												.to_string()
										})
									})?;
								}
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
