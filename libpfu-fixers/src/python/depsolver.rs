use std::fmt::Display;

use anyhow::Result;
use kstring::KString;
use libabbs::apml::value::array::StringArray;
use libpfu::Session;
use log::{debug, error};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
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

		// remove version specifier, platform specifier and feature specifiers
		let req = req
			.split_once([' ', '>', '<', '~', '=', ';', '['])
			.map_or(req, |(req, _)| req);
		Some(KString::from_ref(req))
	}

	/// Normalizes the package name for AOSC naming style.
	pub fn guess_aosc_package_name(&self) -> String {
		self.name.replace('_', "-")
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

pub async fn collect_deps(sess: &Session) -> Result<Vec<Dependency>> {
	debug!("collecting Python dependencies of {:?}", sess.package);

	if let Ok(pyproj_str) = sess.source_fs().await?.read("pyproject.toml").await
	{
		debug!("pyproject.toml found in {:?}", sess.package);
		collect_from_pyproject(&String::from_utf8(pyproj_str.to_vec())?)
	} else if let Ok(req_txt_str) =
		sess.source_fs().await?.read("requirements.txt").await
	{
		debug!("requirements.txt found in {:?}", sess.package);
		collect_from_requirementstxt(&String::from_utf8(req_txt_str.to_vec())?)
	} else {
		Ok(vec![])
	}
}

fn collect_from_pyproject(pyproject_str: &str) -> Result<Vec<Dependency>> {
	let pyproject = toml::from_str::<PyprojectToml>(pyproject_str)?;
	debug!("Parsed pyproject.toml: {pyproject:?}");

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
			name: KString::from_ref(
				backend.split_once('.').map_or(backend.as_str(), |(s, _)| s),
			),
			build_dep: true,
			origin: DependencyOrigin::Pep517BuildBackend,
			raw_req: backend,
		});
	}

	Ok(py_deps)
}

fn collect_from_requirementstxt(req_txt_str: &str) -> Result<Vec<Dependency>> {
	Ok(req_txt_str
		.lines()
		.map(|s| s.split_once('#').map_or(s, |(s, _)| s))
		.map(|s| s.trim())
		.filter(|s| !s.is_empty())
		.filter_map(|raw_req| {
			Dependency::extract_name_from_req(raw_req).map(|name| Dependency {
				name,
				build_dep: false,
				origin: DependencyOrigin::RequirementsTxt,
				raw_req: raw_req.to_string(),
			})
		})
		.collect())
}

#[derive(Debug, Deserialize)]
struct PyprojectToml {
	#[serde(default)]
	project: PyprojectProject,
	#[serde(default, rename = "build-system")]
	build_system: PyprojectBuildSystem,
}

#[derive(Debug, Deserialize, Default)]
struct PyprojectProject {
	#[serde(default)]
	dependencies: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct PyprojectBuildSystem {
	#[serde(default, rename = "build-backend")]
	build_backend: Option<String>,
	#[serde(default, rename = "requires")]
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

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_extract_name_from_req() {
		assert!(
			Dependency::extract_name_from_req("a; platform_system=windows")
				.is_none()
		);
		assert_eq!(Dependency::extract_name_from_req("a").unwrap(), "a");
		assert_eq!(Dependency::extract_name_from_req("a; b").unwrap(), "a");
		assert_eq!(Dependency::extract_name_from_req("a ; b").unwrap(), "a");
		assert_eq!(Dependency::extract_name_from_req("a== 1.0").unwrap(), "a");
		assert_eq!(Dependency::extract_name_from_req("a~= 1.0").unwrap(), "a");
		assert_eq!(Dependency::extract_name_from_req("a>= 1.0").unwrap(), "a");
		assert_eq!(Dependency::extract_name_from_req("a< 1.0").unwrap(), "a");
	}

	#[test]
	fn test_collect_from_pyproject() {
		assert_eq!(
			collect_from_pyproject(
				r##"
[build-system]
requires = ["flit-core"]
build-backend = "flit_core.buildapi"

[project]
dependencies = [
    "packaging>=23.2",
    "wheels; platform_system=windows",
]
"##
			)
			.unwrap(),
			vec![
				Dependency {
					name: "packaging".into(),
					build_dep: false,
					origin: DependencyOrigin::Pep517Dependencies,
					raw_req: "packaging>=23.2".into(),
				},
				Dependency {
					name: "flit-core".into(),
					build_dep: true,
					origin: DependencyOrigin::Pep517BuildRequires,
					raw_req: "flit-core".into(),
				},
				Dependency {
					name: "flit_core".into(),
					build_dep: true,
					origin: DependencyOrigin::Pep517BuildBackend,
					raw_req: "flit_core.buildapi".into(),
				}
			]
		);
	}

	#[test]
	fn test_collect_from_requirementstxt() {
		assert_eq!(
			collect_from_requirementstxt(
				r##"beautifulsoup4==4.5.1
decorator==4.0.10
requests
pip~=100.0
a[b]
"##
			)
			.unwrap(),
			vec![
				Dependency {
					name: "beautifulsoup4".into(),
					build_dep: false,
					origin: DependencyOrigin::RequirementsTxt,
					raw_req: "beautifulsoup4==4.5.1".into(),
				},
				Dependency {
					name: "decorator".into(),
					build_dep: false,
					origin: DependencyOrigin::RequirementsTxt,
					raw_req: "decorator==4.0.10".into(),
				},
				Dependency {
					name: "requests".into(),
					build_dep: false,
					origin: DependencyOrigin::RequirementsTxt,
					raw_req: "requests".into(),
				},
				Dependency {
					name: "pip".into(),
					build_dep: false,
					origin: DependencyOrigin::RequirementsTxt,
					raw_req: "pip~=100.0".into(),
				},
				Dependency {
					name: "a".into(),
					build_dep: false,
					origin: DependencyOrigin::RequirementsTxt,
					raw_req: "a[b]".into(),
				},
			]
		);
	}
}
