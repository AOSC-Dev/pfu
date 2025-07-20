//! pypi handler.

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use log::debug;
use opendal::Operator;
use serde::Deserialize;

use crate::{fetch_tarball, find_alt_fs, http_client};

pub async fn load(package: &str, version: &str) -> Result<Operator> {
	let hints = collect_alt_hints(package).await?;
	for hint in hints {
		if let Some(fs) = find_alt_fs(&hint).await? {
			return Ok(fs);
		}
	}

	let prefix = package
		.chars()
		.next()
		.ok_or_else(|| anyhow!("empty package name"))?;
	let url = format!(
		"https://pypi.io/packages/source/{prefix}/{package}/{package}-{version}.tar.gz"
	);
	fetch_tarball(url).await
}

async fn collect_alt_hints(package: &str) -> Result<Vec<String>> {
	#[derive(Debug, Deserialize)]
	struct PypiProjectJson {
		#[serde(default)]
		info: PypiProjectInfo,
	}
	#[derive(Debug, Deserialize, Default)]
	struct PypiProjectInfo {
		#[serde(default)]
		project_urls: HashMap<String, String>,
	}

	debug!("Fetching PYPI project information: {package}");
	let client = http_client()?;
	let url = format!("https://pypi.org/pypi/{package}/json");
	let proj_json = client
		.execute(client.get(&url).build()?)
		.await?
		.error_for_status()?
		.json::<PypiProjectJson>()
		.await?;

	let mut hints = Vec::new();
	for (k, v) in proj_json.info.project_urls {
		debug!(
			"Found alt hint from PYPI metadata of {package}: {k} -> {v}"
		);
		hints.push(v);
	}

	Ok(hints)
}
