//! Python dependencies checks.

use anyhow::Result;
use async_trait::async_trait;
use libabbs::apml::{lst, value::array::StringArray};
use libpfu::{
	Linter, Session, declare_lint, declare_linter,
	message::{LintMessage, Snippet},
	walk_defines,
};
use log::debug;

use crate::python::depsolver;

declare_linter! {
	pub PYTHON_DEPS_LINTER,
	PythonDepsLinter,
	[
		"python-suggested-dep",
	]
}

declare_lint! {
	pub PYTHON_SUGGEST_DEP_LINT,
	"python-suggested-dep",
	Note,
	"some dependencies may be missed"
}

#[async_trait]
impl Linter for PythonDepsLinter {
	async fn apply(&self, sess: &Session) -> Result<()> {
		if sess.offline {
			return Ok(());
		}
		let mut py_deps = depsolver::collect_deps(sess).await?;
		if py_deps.is_empty() {
			debug!(
				"{:?} does not have any Python dependencies found",
				sess.package
			);
			return Ok(());
		} else {
			debug!(
				"Collected Python dependencies of {:?}: {:?}",
				sess.package, py_deps
			);
		}

		for mut apml in walk_defines(sess) {
			debug!("Checking Python dependencies for {apml:?}");
			let abtype = apml.with_upgraded(|apml| {
				apml.ctx()
					.map(|ctx| ctx.get("ABTYPE").map(|val| val.as_string()))
			})?;
			if let Some(abtype) = abtype
				&& abtype != "pep517" && abtype != "python" {
					debug!(
						"Explicit ABTYPE '{abtype}' is not Python, skipping PEP-517 lints"
					);
					continue;
				}

			let [pkgdep, builddep] = ["PKGDEP", "BUILDDEP"].map(|var| {
				apml.with_upgraded(|apml| {
					apml.ctx().map(|ctx| {
						ctx.get(var)
							.map(|val| val.as_string())
							.unwrap_or_default()
					})
				})
				.map(StringArray::from)
			});
			let (mut pkgdep, mut builddep) = (pkgdep?, builddep?);
			let mut pkgdep_dirty = false;

			for dep in &mut py_deps {
				if let Some(prov_pkg) =
					depsolver::find_system_package(dep, &pkgdep, &builddep)
						.await?
				{
					if pkgdep.contains(&prov_pkg)
						|| (dep.build_dep && builddep.contains(&prov_pkg))
					{
						continue;
					}

					apml.with_upgraded(|apml| {
						LintMessage::new(PYTHON_SUGGEST_DEP_LINT)
							.snippet(Snippet::new_variable(
								sess,
								apml,
								if dep.build_dep {
									"BUILDDEP"
								} else {
									"PKGDEP"
								},
							))
							.note(format!(
								"package '{prov_pkg}' provides {} dependency '{}'",
								if dep.build_dep { "build" } else { "runtime" },
								dep.name,
							))
							.note(format!(
								"requirement '{}' found in {}",
								dep.raw_req, dep.origin,
							))
							.emit(sess);

						if !sess.dry {
							if !dep.build_dep {
								pkgdep.push(prov_pkg.clone());
							} else {
								builddep.push(prov_pkg.clone());
							}
							pkgdep_dirty = true;
						}
					});
				}
			}
			if pkgdep_dirty {
				apml.with_upgraded(|apml| {
					apml.with_editor(|apml| {
						apml.replace_var_lst(
							"PKGDEP",
							lst::VariableValue::String(pkgdep.print().into()),
						);
						apml.replace_var_lst(
							"BUILDDEP",
							lst::VariableValue::String(builddep.print().into()),
						);
					})
				});
			}
		}
		Ok(())
	}
}
