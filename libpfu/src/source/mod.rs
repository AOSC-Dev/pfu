//! Source-code access layers.

use std::{fs, sync::LazyLock};

use anyhow::{Context, Result};
use libabbs::apml::{
	ApmlContext,
	value::{array::StringArray, union::Union},
};
use log::{info, warn};
use opendal::{
	Operator,
	layers::RetryLayer,
	services::{Github, Memory},
};
use regex::Regex;

use super::Session;

static REGEX_GH_URL: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(
		r##"http(s|)://github\.com/(?<user>[a-zA-Z_-]+)/(?<repo>[a-zA-Z_-]+)"##,
	)
	.unwrap()
});

/// Initializes the source code access for a context.
pub async fn open(sess: &Session) -> Result<Operator> {
	let spec_src = fs::read_to_string(sess.package.join("spec"))
		.context("Cannot read spec file")?;
	let spec_ctx = ApmlContext::eval_source(&spec_src)?;
	let srcs_str = spec_ctx.read("SRCS").into_string();
	let srcs = StringArray::from(srcs_str);

	if srcs.len() == 1 {
		let src = srcs[0].clone();
		let un = if src.starts_with("https://") || src.starts_with("http://") {
			Union::try_from(format!("tbl::{}", src).as_str())?
		} else {
			Union::try_from(src.as_str())?
		};

		match un.tag.as_str() {
			"tarball" | "tbl" => {
				if let Some(url) = un.argument {
					if let Some(cap) = REGEX_GH_URL.captures(&url) {
						let owner = &cap["user"];
						let repo = &cap["repo"];
						info!(
							"recognized GitHub repository {}/{} for {:?}",
							owner, repo, sess.package
						);
						return Ok(Operator::new(
							Github::default().owner(owner).repo(repo),
						)?
						.layer(RetryLayer::new())
						.finish());
					}
				}
			}
			"git" => {
				if let Some(url) = un.argument {
					if let Some(cap) = REGEX_GH_URL.captures(&url) {
						let owner = &cap["user"];
						let repo = &cap["repo"];
						info!(
							"recognized GitHub repository {}/{} for {:?}",
							owner, repo, sess.package
						);
						return Ok(Operator::new(
							Github::default().owner(owner).repo(repo),
						)?
						.layer(RetryLayer::new())
						.finish());
					}
				}
			}
			_ => {
				warn!("unsupported source type: {}", un.tag);
			}
		}
	} else {
		warn!("multiple sources are not supported yet");
	}
	Ok(Operator::new(Memory::default())?.finish())
}
