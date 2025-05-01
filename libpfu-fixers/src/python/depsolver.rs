use std::fmt::Display;

use anyhow::Result;
use kstring::KString;
use libabbs::apml::value::array::StringArray;
use libpfu::Session;
use log::{debug, error};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Dependency {
	pub name: KString,
	pub build_dep: bool,
	pub origin: DependencyOrigin,
	pub raw_req: String,
}

impl Dependency {
	/// Extracts the package name out from a Python dependency requirement.
	pub fn extract_name_from_req(req: &str) -> Option<KString> {
		// exclude windows and OSX-only dependencies
		if let Some((_, cond)) = req.split_once(';') {
			let cond = cond.to_ascii_lowercase();
			if cond.contains("platform_system")
				&& (cond.contains("windows") || cond.contains("darwin"))
			{
				return None;
			}
		}

		// remove version specifier and platform specifier
		let req = req
			.split_once([' ', '>', '<', '~', '=', ';'])
			.map_or(req, |(req, _)| req);
		Some(KString::from_ref(req))
	}

	/// Normalizes the package name for AOSC naming style.
	pub fn guess_aosc_package_name(&self) -> String {
		self.name.replace('_', "-")
	}
}

#[derive(Debug, Clone, Copy)]
pub enum DependencyOrigin {
	RequirementsTxt,
	Pep517Dependencies,
	Pep517BuildRequires,
	Pep517BuildBackend,
}

impl Display for DependencyOrigin {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			DependencyOrigin::RequirementsTxt => {
				f.write_str("requirements.txt")
			}
			DependencyOrigin::Pep517Dependencies => {
				f.write_str("project.dependencies from pyproject.toml")
			}
			DependencyOrigin::Pep517BuildRequires => {
				f.write_str("build-system.requires from pyproject.toml")
			}
			DependencyOrigin::Pep517BuildBackend => {
				f.write_str("build-system.build-backend from pyproject.toml")
			}
		}
	}
}

// TODO: support requirements.txt
pub async fn collect_deps(sess: &Session) -> Result<Vec<Dependency>> {
	if let Ok(pyproj_str) = sess.source_fs().await?.read("pyproject.toml").await
	{
		debug!("pyproject.toml found for {:?}", sess.package);
		collect_from_pyproject(&String::from_utf8(pyproj_str.to_vec())?)
	} else {
		Ok(vec![])
	}
}

fn collect_from_pyproject(pyproject_str: &str) -> Result<Vec<Dependency>> {
	let pyproject = toml::from_str::<PyprojectToml>(&pyproject_str)?;
	debug!("Parsed pyproject.toml: {:?}", pyproject);

	let mut py_deps = vec![];
	for raw_req in pyproject.project.dependencies {
		if let Some(name) = Dependency::extract_name_from_req(&raw_req) {
			py_deps.push(Dependency {
				name,
				build_dep: false,
				origin: DependencyOrigin::Pep517Dependencies,
				raw_req,
			});
		}
	}
	for raw_req in pyproject.build_system.requires {
		if let Some(name) = Dependency::extract_name_from_req(&raw_req) {
			py_deps.push(Dependency {
				name,
				build_dep: true,
				origin: DependencyOrigin::Pep517BuildRequires,
				raw_req,
			});
		}
	}
	if let Some(backend) = pyproject.build_system.build_backend {
		py_deps.push(Dependency {
			name: KString::from_ref(&backend),
			build_dep: true,
			origin: DependencyOrigin::Pep517BuildBackend,
			raw_req: backend,
		});
	}

	Ok(py_deps)
}

#[derive(Debug, Deserialize)]
#[serde(rename = "kebab-case")]
struct PyprojectToml {
	#[serde(default)]
	project: PyprojectProject,
	#[serde(default)]
	build_system: PyprojectBuildSystem,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename = "kebab-case")]
struct PyprojectProject {
	#[serde(default)]
	dependencies: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename = "kebab-case")]
struct PyprojectBuildSystem {
	#[serde(default)]
	build_backend: Option<String>,
	#[serde(default)]
	requires: Vec<String>,
}

/// Finds the system package which provides a certain Python package.
pub async fn find_system_package(
	dep: &Dependency,
	pkgdep: &StringArray,
	builddep: &StringArray,
) -> Result<Option<String>> {
	let find_dep = |pkg: &str| {
		if pkgdep.iter().any(|dep| dep == pkg) {
			debug!(
				"Matched Python dependency package in PKGDEP: {} -> {}",
				dep.name, pkg
			);
			return true;
		}
		if dep.build_dep && builddep.iter().any(|dep| dep == pkg) {
			debug!(
				"Matched Python dependency package in BUILDDEP: {} -> {}",
				dep.name, pkg
			);
			return true;
		}
		false
	};

	// Find in current dependencies
	let aosc_package_name = dep.guess_aosc_package_name();
	if find_dep(&aosc_package_name) {
		debug!(
			"Matched Python dependency through name-normalization: {}",
			dep.name
		);
		return Ok(Some(aosc_package_name));
	}

	// Find in apt database
	let mut found = None;
	match oma_contents::searcher::search(
		"/var/lib/apt/lists",
		oma_contents::searcher::Mode::Provides,
		&if dep.name.contains('-') {
			format!("/site-packages/{}/", dep.name.replace('-', "_"))
		} else {
			format!("/site-packages/{}/", dep.name)
		},
		|(pkg, path)| {
			if path.starts_with("/usr/lib/python") {
				found = Some(pkg)
			}
		},
	) {
		Ok(()) => {
			match &found {
				Some(pkg) => debug!(
					"Found system package for Python package: {} -> {}",
					dep.name, pkg
				),
				None => debug!(
					"No system package was found for Python package: {}",
					dep.name
				),
			}
			Ok(found)
		}
		Err(err) => {
			error!(
				"Failed to find system package for Python package {}: {:?}",
				dep.name, err
			);
			Ok(None)
		}
	}
}
