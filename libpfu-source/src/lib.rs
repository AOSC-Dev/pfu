//! Source-code access layers.

use std::{io::Read, sync::LazyLock};

use anyhow::{Result, anyhow, bail};
use bytes::Buf;
use futures::executor::block_on;
use libabbs::apml::value::{array::StringArray, union::Union};
use log::{debug, info, warn};
use opendal::{
	Operator,
	layers::RetryLayer,
	services::{Github, Memory},
};
use regex::Regex;
use reqwest::ClientBuilder;
use tempfile::tempfile;

static REGEX_GH_URL: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(
		r##"http(s|)://github\.com/(?<user>[a-zA-Z_-]+)/(?<repo>[a-zA-Z_-]+)"##,
	)
	.unwrap()
});

/// Initializes the source code access for a context.
pub async fn open(srcs: String) -> Result<Operator> {
	let srcs = StringArray::from(srcs);

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
						debug!(
							"recognized GitHub repository {}/{} from {}",
							owner, repo, url
						);
						return Ok(Operator::new(
							Github::default().owner(owner).repo(repo),
						)?
						.layer(RetryLayer::new())
						.finish());
					}
					return fetch_tarball(url).await;
				}
			}
			"git" => {
				if let Some(url) = un.argument {
					if let Some(cap) = REGEX_GH_URL.captures(&url) {
						let owner = &cap["user"];
						let repo = &cap["repo"];
						debug!(
							"recognized GitHub repository {}/{} from {}",
							owner, repo, url
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

async fn fetch_tarball(url: String) -> Result<Operator> {
	info!("Downloading tarball: {}", url);
	let client = ClientBuilder::new()
		.user_agent(format!(
			"libpfu/{} (https://github.com/AOSC-Dev/pfu)",
			env!("CARGO_PKG_VERSION")
		))
		.build()?;
	let resp = client
		.execute(client.get(&url).build()?)
		.await?
		.error_for_status()?;

	let reader = resp.bytes().await?.reader();
	let fs = block_on(async { load_compressed_tarball(&url, reader).await })?;
	Ok(fs)
}

async fn load_compressed_tarball(
	name: &str,
	reader: impl Read,
) -> Result<Operator> {
	if name.ends_with(".tar") {
		debug!("Recognized bare tarball");
		load_tarball(reader).await
	} else if name.ends_with(".tar.gz") || name.ends_with(".tar.gzip") {
		debug!("Recognized tarball + gzip");
		let reader = flate2::read::GzDecoder::new(reader);
		load_tarball(reader).await
	} else if name.ends_with(".tar.xz") {
		debug!("Recognized tarball + XZ");
		let reader = xz2::read::XzDecoder::new(reader);
		load_tarball(reader).await
	} else if name.ends_with(".tar.zst") || name.ends_with(".tar.zstd") {
		debug!("Recognized tarball + zstd");
		let reader = zstd::Decoder::new(reader)?;
		load_tarball(reader).await
	} else if name.ends_with(".tar.bz")
		|| name.ends_with(".tar.bz2")
		|| name.ends_with(".tar.bzip")
	{
		debug!("Recognized tarball + bz");
		let reader = bzip2::read::BzDecoder::new(reader);
		load_tarball(reader).await
	} else {
		bail!("unsupported archive type")
	}
}

async fn load_tarball(mut reader: impl Read) -> Result<Operator> {
	let fs = Operator::new(Memory::default())?.finish();

	let mut temp = tempfile()?;
	std::io::copy(&mut reader, &mut temp)?;

	let mut tar = tar::Archive::new(temp);
	for entry in tar.entries()? {
		let mut entry = entry?;
		if entry.header().entry_type() == tar::EntryType::Directory {
			fs.create_dir(
				entry
					.path()?
					.to_str()
					.ok_or_else(|| anyhow!("invalid dir name in tarball"))?,
			)
			.await?;
		} else {
			let path = entry.path()?.to_path_buf();
			if let Some(parent) = path.parent() {
				fs.create_dir(
					parent.to_str().ok_or_else(|| {
						anyhow!("invalid dir name in tarball")
					})?,
				)
				.await?;
			}
			let mut buf = Vec::with_capacity(entry.size() as usize);
			entry.read_to_end(&mut buf)?;
			fs.write(
				path.to_str()
					.ok_or_else(|| anyhow!("invalid filename in tarball"))?,
				buf,
			)
			.await?;
		}
	}

	Ok(fs)
}
