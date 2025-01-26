//! `CHKUPDATE` checks.

use std::cell::OnceCell;

use anyhow::Result;
use async_trait::async_trait;
use itertools::Itertools;
use libabbs::apml::{ast, lst, value::array::StringArray};
use libpfu::{
	Linter, Session, declare_lint, declare_linter,
	message::{LintMessage, Snippet},
	walk_defines,
};
use log::{debug, error};
use serde::Deserialize;

declare_linter! {
	pub PEP517_LINTER,
	Pep517Linter,
	[
		"upgrade-to-pep517",
		"pep517-nopython2",
		"pep517-python2-dep",
		"pep517-python3-dep",
	]
}

declare_lint! {
	pub UPGRADE_TO_PEP517_LINT,
	"upgrade-to-pep517",
	Warning,
	"use PEP-517 build backend"
}

declare_lint! {
	pub PEP517_NOPYTHON2_LINT,
	"pep517-nopython2",
	Error,
	"PEP-517 build template requires NOPYTHON2=1"
}

declare_lint! {
	pub PEP517_PYTHON2_DEP_LINT,
	"pep517-python2-dep",
	Warning,
	"python-2 should not be included in dependencies of PEP-517 package"
}

declare_lint! {
	pub PEP517_PYTHON3_DEP_LINT,
	"pep517-python3-dep",
	Error,
	"python-3 must be included as a runtime dependency of PEP-517 package"
}

declare_lint! {
	pub PEP517_SUGGEST_DEP_LINT,
	"pep517-suggested-dep",
	Note,
	"the package may misses some dependencies (found from pyproject.toml)"
}

#[async_trait]
impl Linter for Pep517Linter {
	async fn apply(&self, sess: &Session) -> Result<()> {
		if let Ok(pyproj_str) =
			sess.source_fs().await?.read("pyproject.toml").await
		{
			debug!(
				"pyproject.toml found, checking PEP-517 lints for {:?}",
				sess.package
			);

			let pyproj_str = String::from_utf8(pyproj_str.to_vec())?;
			let pyproj = toml::from_str::<PyprojectToml>(&pyproj_str)?;
			debug!(
				"Loaded pyproject.toml for {:?}: {:?}",
				sess.package, pyproj
			);
			let mut py_deps = vec![];
			for dep in pyproj.project.dependencies {
				py_deps.push((false, dep));
			}
			for dep in pyproj.build_system.requires {
				py_deps.push((true, dep));
			}
			if let Some(backend) = pyproj.build_system.build_backend {
				py_deps.push((true, backend));
			}
			debug!(
				"Collected Python dependencies for {:?}: {:?}",
				sess.package, py_deps
			);
			let mut py_deps = py_deps
				.into_iter()
				.filter_map(|(is_build, dep)| {
					if let Some((dep, cond)) = dep.split_once(';') {
						let cond = cond.to_ascii_lowercase();
						if cond.contains("platform_system")
							&& cond.contains("windows")
						{
							return None;
						}
						Some((is_build, dep.to_string()))
					} else {
						Some((is_build, dep))
					}
				})
				.map(|(is_build, dep)| {
					// remove version specifier
					for op in ['<', '>', '='] {
						if let Some((dep, _)) = dep.split_once(op) {
							return (is_build, dep.to_string());
						}
					}
					(is_build, dep)
				})
				.map(|(is_build, dep)| {
					let uniformed_dep =
						dep.replace('_', "-").to_ascii_lowercase();
					(
						is_build,
						dep,
						uniformed_dep,
						OnceCell::<Option<String>>::new(),
					)
				})
				.collect_vec();

			for mut apml in walk_defines(sess) {
				let abtype = apml.with_upgraded(|apml| {
					apml.ctx()
						.map(|ctx| ctx.get("ABTYPE").map(|val| val.as_string()))
				})?;
				if let Some(abtype) = abtype {
					if abtype == "python" {
						apml.with_upgraded(|apml| {
							LintMessage::new(UPGRADE_TO_PEP517_LINT)
								.note("remove ABTYPE=python to allow automatic template detection".to_string())
								.snippet(Snippet::new_variable(sess, apml, "ABTYPE"))
								.emit(sess);
							if !sess.dry {
								apml.with_editor(|apml| {
									apml.remove_var(
										apml.find_var_index("ABTYPE").unwrap(),
									)
								})
							}
						})
					}
				}

				let nopy2 = apml.with_upgraded(|apml| {
					apml.ctx()
						.map(|ctx| ctx.read("NOPYTHON2").into_string() == "1")
				})?;
				if !nopy2 {
					LintMessage::new(PEP517_NOPYTHON2_LINT)
						.snippet(Snippet::new_index(sess, &apml, 0))
						.emit(sess);
					if !sess.dry {
						apml.with_upgraded(|apml| {
							apml.with_editor(|apml| {
								apml.append_var_ast(
									"NOPYTHON2",
									&ast::VariableValue::String(
										ast::Text::from("1"),
									),
									Some("ABTYPE"),
								);
							})
						})
					}
				}

				let pkgdep = apml.with_upgraded(|apml| {
					apml.ctx().map(|ctx| {
						ctx.get("PKGDEP")
							.map(|val| val.as_string())
							.unwrap_or_default()
					})
				})?;
				let mut pkgdep = StringArray::from(pkgdep);
				let mut pkgdep_dirty = false;
				let builddep = apml.with_upgraded(|apml| {
					apml.ctx().map(|ctx| {
						ctx.get("BUILDDEP")
							.map(|val| val.as_string())
							.unwrap_or_default()
					})
				})?;
				let mut builddep = StringArray::from(builddep);
				if pkgdep.iter().any(|dep| dep == "python-2") {
					apml.with_upgraded(|apml| {
						LintMessage::new(PEP517_PYTHON2_DEP_LINT)
							.snippet(Snippet::new_variable(
								sess, apml, "PKGDEP",
							))
							.emit(sess);
					});
					if !sess.dry {
						let pos = pkgdep
							.iter()
							.position(|dep| dep == "python-2")
							.unwrap();
						pkgdep.remove(pos);
						pkgdep_dirty = true;
					}
				}
				if !pkgdep.iter().any(|dep| dep == "python-3") {
					apml.with_upgraded(|apml| {
						LintMessage::new(PEP517_PYTHON3_DEP_LINT)
							.snippet(Snippet::new_variable(
								sess, apml, "PKGDEP",
							))
							.emit(sess);
					});
					if !sess.dry {
						pkgdep.push("python-3".to_string());
						pkgdep_dirty = true;
					}
				}
				for (is_build, dep, uniformed_dep, prov_pkg) in &mut py_deps {
					let find_dep = |pkg: &str| {
						if pkgdep.iter().any(|dep| dep == pkg) {
							debug!(
								"{:?}: Matched dependency package in PKGDEP: {} -> {}",
								apml, dep, pkg
							);
							return true;
						}
						if *is_build && builddep.iter().any(|dep| dep == pkg) {
      								debug!(
      									"{:?}: Matched dependency package in BUILDDEP: {} -> {}",
      									apml, dep, pkg
      								);
      								return true;
      							}
						false
					};
					if find_dep(uniformed_dep) {
						debug!(
							"{:?}: Matched Python dependency through name-normalization: {}",
							apml, dep
						);
						continue;
					}
					let prov_pkg = prov_pkg.get_or_init(|| {
						let mut found = None;
						match oma_contents::searcher::search(
							"/var/lib/apt/lists",
							oma_contents::searcher::Mode::Provides,
							&format!("/site-packages/{}/", dep),
							|(pkg, path)| {
								if path.starts_with("/usr/lib/python") {
									found = Some(pkg)
								}
							},
						) {
							Ok(()) => {
								match &found {
									Some(pkg) => debug!(
										"Found provider package for Python package: {} -> {}",
										dep, pkg
									),
									None => debug!(
										"Unable to find provider package for Python package: {}",
										dep
									),
								}
								found
							}
							Err(err) => {
								error!(
									"Failed to search provider package for Python package {}: {:?}",
									dep, err
								);
								None
							}
						}
					});
					if let Some(prov_pkg) = prov_pkg {
						if find_dep(prov_pkg) {
							continue;
						}

						apml.with_upgraded(|apml| {
							if !*is_build {
								LintMessage::new(PEP517_SUGGEST_DEP_LINT)
									.snippet(Snippet::new_variable(
										sess, apml, "PKGDEP",
									))
									.note(format!(
										"package {} provides runtime dependency {}",
										prov_pkg, dep
									))
									.emit(sess);
								if !sess.dry {
									pkgdep.push(prov_pkg.clone());
									pkgdep_dirty = true;
								}
							} else {
								LintMessage::new(PEP517_SUGGEST_DEP_LINT)
									.snippet(Snippet::new_variable(
										sess, apml, "BUILDDEP",
									))
									.note(format!(
										"package {} provides build dependency {}",
										prov_pkg, dep
									))
									.emit(sess);
								if !sess.dry {
									builddep.push(prov_pkg.clone());
									pkgdep_dirty = true;
								}
							}
						});
					}
				}
				if pkgdep_dirty {
					apml.with_upgraded(|apml| {
						apml.with_editor(|apml| {
							apml.replace_var_lst(
								"PKGDEP",
								lst::VariableValue::String(
									pkgdep.print().into(),
								),
							);
							apml.replace_var_lst(
								"BUILDDEP",
								lst::VariableValue::String(
									builddep.print().into(),
								),
							);
						})
					});
				}
			}
		}
		Ok(())
	}
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
