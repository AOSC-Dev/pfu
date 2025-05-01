//! Autobuild arch-group overrides checks.

use anyhow::Result;
use async_trait::async_trait;
use libabbs::apml::ast;
use libpfu::{
	Linter, Session, declare_lint, declare_linter,
	message::{LintMessage, Snippet},
	walk_apml,
};
use log::debug;

declare_linter! {
	pub ARCH_GROUP_LINTER,
	ArchGroupLinter,
	[
		"missing-archgroup",
		"redundant-arch-overrides",
	]
}

declare_lint! {
	pub MISSING_ARCHGROUP_LINT,
	"missing-archgroup",
	Warning,
	"some arch-groups are missed from arch-overrides"
}

declare_lint! {
	pub REDUNDANT_ARCH_OVERRIDES_LINT,
	"redundant-arch-overrides",
	Warning,
	"some arch-overrides are redundant"
}

/// Architecture-overridable variables used by ACBS
const ACBS_VARIABLES: &[&str] = &["PKGDEP", "BUILDDEP"];

#[async_trait]
impl Linter for ArchGroupLinter {
	async fn apply(&self, sess: &Session) -> Result<()> {
		for mut apml in walk_apml(sess) {
			debug!("Looking for redundant arch-group variables in {:?}", apml);
			apml.with_upgraded(|apml| {
				let mut arch_overrides = vec![];
				'vars: for var in &apml.ast()?.0 {
					let (var_name, target) =
						if let Some(v) = var.name.split_once("__") {
							v
						} else {
							continue 'vars;
						};

					let mut included_vars = vec![];
					match &var.value {
						ast::VariableValue::String(text) => {
							for word in &text.0 {
								match word {
									ast::Word::Literal(text)
										if text.trim().is_empty() => {}
									ast::Word::Variable(exp)
										if exp.modifier.is_none() =>
									{
										included_vars
											.push(exp.name.to_string());
									}
									_ => continue 'vars,
								}
							}
						}
						ast::VariableValue::Array(elements) => {
							for element in elements {
								match element {
									ast::ArrayElement::ArrayInclusion(name) => {
										included_vars.push(name.to_string());
									}
									_ => continue 'vars,
								}
							}
						}
					}
					let mut included_groups = vec![];
					for var in &included_vars {
						let (include_name, group) =
							if let Some(v) = var.split_once("__") {
								v
							} else {
								continue 'vars;
							};
						if include_name != var_name {
							continue 'vars;
						}
						included_groups.push(group.to_ascii_lowercase());
					}
					debug!(
						"Variable override '{}' included groups: {:?}",
						var.name, included_groups
					);
					arch_overrides.push((
						var.name.to_string(),
						var_name.to_string(),
						target.to_ascii_lowercase(),
						included_groups,
					));
				}

				for (var_name, base_name, target, groups) in arch_overrides {
					for (archgroup, targets) in &sess.ab4_data.arch_groups {
						if targets.contains(target.as_str())
							&& !groups.contains(&archgroup.to_string())
						{
							let group_var_name = format!(
								"{}__{}",
								base_name,
								archgroup.to_ascii_uppercase()
							);
							if apml.ctx()?.contains_var(&group_var_name) {
								LintMessage::new(MISSING_ARCHGROUP_LINT)
									.note(format!(
										"'{group_var_name}' is defined but not included in '{var_name}'",
									))
									.note(format!(
										"'{target}' is in arch-group '{archgroup}'",
									))
									.snippet(Snippet::new_variable(
										sess, apml, &var_name,
									))
									.emit(sess);
							}
						}
					}

					if !ACBS_VARIABLES.contains(&base_name.as_str())
						&& groups.iter().all(|group| {
							sess.ab4_data
								.arch_groups
								.get(group.as_str())
								.map_or(false, |targets| {
									targets.contains(target.as_str())
								})
						}) {
						LintMessage::new(REDUNDANT_ARCH_OVERRIDES_LINT)
							.snippet(Snippet::new_variable(
								sess, apml, &var_name,
							))
							.emit(sess);
						if !sess.dry {
							apml.with_editor(|editor| {
								if let Some(index) =
									editor.find_var_index(var_name)
								{
									editor.remove_var(index);
								}
							});
						}
					}
				}

				Ok::<_, anyhow::Error>(())
			})?;
		}
		Ok(())
	}
}
