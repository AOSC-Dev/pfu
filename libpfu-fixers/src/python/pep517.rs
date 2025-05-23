//! PEP-517 checks.

use anyhow::Result;
use async_trait::async_trait;
use libabbs::apml::{ast, lst, value::array::StringArray};
use libpfu::{
	Linter, Session, declare_lint, declare_linter,
	message::{LintMessage, Snippet},
	walk_defines,
};
use log::debug;

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

#[async_trait]
impl Linter for Pep517Linter {
	async fn apply(&self, sess: &Session) -> Result<()> {
		if !sess.offline
			&& sess.source_fs().await?.exists("pyproject.toml").await?
		{
			debug!(
				"pyproject.toml found, checking PEP-517 lints for {:?}",
				sess.package
			);

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
					} else if abtype != "pep517" {
						debug!(
							"Explicit ABTYPE '{abtype}' is not pep517, skipping PEP-517 lints"
						);
						continue;
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
									"NOPYTHON2".to_string(),
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
				if pkgdep_dirty {
					apml.with_upgraded(|apml| {
						apml.with_editor(|apml| {
							apml.replace_var_lst(
								"PKGDEP",
								lst::VariableValue::String(
									pkgdep.print().into(),
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
