//! `SRCS` checks.

use std::sync::LazyLock;

use anyhow::Result;
use async_trait::async_trait;
use libabbs::apml::{
	lst,
	value::{array::StringArray, union::Union},
};
use libpfu::{
	Linter, Session, declare_lint, declare_linter,
	message::{LintMessage, Snippet},
	walk_apml,
};
use log::{debug, warn};
use regex::Regex;

declare_linter! {
	pub SRCS_LINTER,
	SrcsLinter,
	[
		"unknown-fetch-tag",
		"prefer-specific-src-handler",
		"insecure-src-url",
		"https-unsupported-src",
	]
}

declare_lint! {
	pub UNKNOWN_FETCH_TAG_LINT,
	"unknown-fetch-tag",
	Error,
	"unknown handler found in SRCS"
}

declare_lint! {
	pub PREFER_SPECIFIC_SRC_HANDLER_LINT,
	"prefer-specific-src-handler",
	Warning,
	"use more-specific handler for SRCS"
}

declare_lint! {
	pub INSECURE_SRC_URL_LINT,
	"insecure-src-url",
	Warning,
	"replace insecure http:// links with https://"
}

declare_lint! {
	pub HTTPS_UNSUPPORTED_SRC_LINT,
	"https-unsupported-src",
	Info,
	"source server supports http:// only, which is insecure"
}

const REGEX_TBL: &str = "(tarball|tbl)::";
const REGEX_VERSION_TAR: &str = r##"(?P<version>\$VER|[a-zA-Z0-9\.]*\$\{[^}]+\}|[^\.]+)\.tar(\.gz|\.xz|\.bz2|\.bz|\.zstd|\.zst|)"##;

static REGEX_PYPI: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r##"http(s|)://(?:pypi\.io|pypi\.org|pypi\.python\.org)/packages/source/[A-Za-z]/(?P<name>[A-Za-z0-9\._\-]+)/([A-Za-z0-9\._\-]+)"##).unwrap()
});
static REGEX_PYPI_FULL: LazyLock<Regex> = LazyLock::new(|| {
	let regex = format!(
		"{}{}{}",
		REGEX_TBL,
		r##"http(s|)://(?:pypi\.io|pypi\.org|pypi\.python\.org)/packages/source/[A-Za-z]/(?P<name>[A-Za-z0-9\._\-]+)/([A-Za-z0-9\._\-]+)-"##,
		REGEX_VERSION_TAR
	);
	Regex::new(&regex).unwrap()
});
static REGEX_GH_TAR: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(
		r##"http(s|)://github\.com/(?<user>[a-zA-Z_-]+)/(?<repo>[a-zA-Z_-]+)/archive/"##,
	)
	.unwrap()
});
static REGEX_GH_TAR_FULL: LazyLock<Regex> = LazyLock::new(|| {
	let regex = format!(
		"{}{}{}",
		REGEX_TBL,
		r##"http(s|)://github\.com/(?<user>[a-zA-Z_-]+)/(?<repo>[a-zA-Z_-]+)/archive/(?:refs/tags/)"##,
		REGEX_VERSION_TAR
	);
	Regex::new(&regex).unwrap()
});

#[async_trait]
impl Linter for SrcsLinter {
	async fn apply(&self, sess: &Session) -> Result<()> {
		for mut apml in walk_apml(sess) {
			debug!("Looking for less-specific handlers in SRCS in {apml:?}");
			let srcs = apml.with_upgraded(|apml| {
				apml.ctx().map(|ctx| ctx.read("SRCS").into_string())
			});
			let mut srcs = StringArray::from(srcs?);

			for (idx, src) in srcs.iter_mut().enumerate() {
				let un = if src.starts_with("https://")
					|| src.starts_with("http://")
					|| !src.contains("::")
				{
					Union::try_from(format!("tbl::{src}").as_str())?
				} else {
					Union::try_from(src.as_str())?
				};

				if let Some(url) = &un.argument
					&& let Some(domain_path) = url.strip_prefix("http://") {
						let mut https_valid = true;

						if !sess.offline {
							let https_url = url.replace("http://", "https://");
							debug!("Checking HTTPS URL: {https_url}");
							let client = sess.http_client()?;
							if let Ok(status) = client
								.head(https_url)
								.send()
								.await
								.map(|resp| resp.status())
							{
								https_valid = status.is_success()
									|| status.is_redirection();
							} else {
								https_valid = false;
							}
						}

						if https_valid {
							apml.with_upgraded(|apml| {
								LintMessage::new(INSECURE_SRC_URL_LINT)
									.note(format!(
										"source {idx} should use https://"
									))
									.snippet(Snippet::new_variable(
										sess, apml, "SRCS",
									))
									.emit(sess);
								if !sess.dry {
									apml.with_text(|text| {
										let domain = domain_path
											.split_once('/')
											.unzip()
											.0
											.unwrap_or(domain_path);
										text.replace(
											&format!("http://{domain}"),
											&format!("https://{domain}"),
										)
									})?;
								}
								Ok::<(), anyhow::Error>(())
							})?;
						} else {
							apml.with_upgraded(|apml| {
								LintMessage::new(HTTPS_UNSUPPORTED_SRC_LINT)
									.note(format!(
										"source {idx} does not support https://"
									))
									.snippet(Snippet::new_variable(
										sess, apml, "SRCS",
									))
									.emit(sess);
							});
						}
					}

				match un.tag.to_ascii_lowercase().as_str() {
					"tarball" | "tbl" => {
						if let Some(arg) = un.argument {
							if let Some(cap) = REGEX_PYPI.captures(&arg) {
								apml.with_upgraded(|apml| {
									LintMessage::new(
										PREFER_SPECIFIC_SRC_HANDLER_LINT,
									)
									.note(format!(
										"source {} should be replaced with pypi::{}",
										idx, &cap["name"],
									))
									.snippet(Snippet::new_variable(
										sess, apml, "SRCS",
									))
									.emit(sess);
								});
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
								apml.with_upgraded(|apml| {
									LintMessage::new(
										PREFER_SPECIFIC_SRC_HANDLER_LINT,
									)
									.note(format!(
										"source {} should be replaced with git::https://github.com/{}/{}.git",
										idx, &cap["user"], &cap["repo"],
									))
									.snippet(Snippet::new_variable(
										sess, apml, "SRCS",
									))
									.emit(sess);
								});
								if !sess.dry {
									apml.with_upgraded(|apml| {
										apml.with_text(|text| {
											REGEX_GH_TAR_FULL
												.replace(
													&text,
													"git::commit=tags/${version}::https://github.com/${user}/${repo}.git",
												)
												.to_string()
										})?;
										let mut chksums = StringArray::from(apml.ctx()?.read("CHKSUMS").into_string());
										match chksums.get_mut(idx) {
											Some(chksum) => *chksum = "SKIP".to_string(),
											None => warn!("failed to replace CHKSUMS entry"),
										}
										apml.with_editor(|editor| {
											editor.replace_var_lst("CHKSUMS", lst::VariableValue::String(chksums.print_expanded().into()));
										});
										Ok::<_, anyhow::Error>(())
									})?;
								}
							}
						}
					}
					"git" | "svn" | "bzr" | "hg" | "fossil" | "file"
					| "pypi" | "none" => {}
					_ => {
						apml.with_upgraded(|apml| {
							LintMessage::new(UNKNOWN_FETCH_TAG_LINT)
								.note(format!(
									"source {} with tag {} is unsupported",
									idx, un.tag
								))
								.snippet(Snippet::new_variable(
									sess, apml, "SRCS",
								))
								.emit(sess);
						});
					}
				}
			}
		}
		Ok(())
	}
}
