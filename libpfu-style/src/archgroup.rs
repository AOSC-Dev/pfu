//! Autobuild arch-group overrides checks.

use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use kstring::KString;
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
		"acbs-arch-groups",
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

declare_lint! {
	pub ACBS_ARCH_GROUPS_LINT,
	"acbs-arch-groups",
	Error,
	"ACBS does not support arch-groups"
}

/// Architecture-overridable variables used by ACBS
const ACBS_VARIABLES: &[&str] = &["PKGDEP", "BUILDDEP"];

#[async_trait]
impl Linter for ArchGroupLinter {
	async fn apply(&self, sess: &Session) -> Result<()> {
		for mut apml in walk_apml(sess) {
			apml.with_upgraded(|apml| {
				debug!("Looking for arch-overrides variables in {:?}", apml);
				let mut arch_overrides = HashMap::new();
				let mut arch_groups = vec![];
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
									_ => {
										arch_groups.push((
											var.name.to_string(),
											KString::from_ref(var_name),
											target.to_ascii_lowercase(),
											false,
										));
										continue 'vars;
									}
								}
							}
						}
						ast::VariableValue::Array(elements) => {
							for element in elements {
								match element {
									ast::ArrayElement::ArrayInclusion(name) => {
										included_vars.push(name.to_string());
									}
									_ => {
										arch_groups.push((
											var.name.to_string(),
											KString::from_ref(var_name),
											target.to_ascii_lowercase(),
											true,
										));
										continue 'vars;
									}
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
					arch_overrides.insert(
						(
							KString::from_ref(var_name),
							target.to_ascii_lowercase(),
						),
						(var.name.to_string(), included_groups),
					);
				}

				for ((base_name, target), (var_name, groups)) in &arch_overrides
				{
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
										sess, apml, var_name,
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
								.is_some_and(|targets| {
									targets.contains(target.as_str())
								})
						}) {
						LintMessage::new(REDUNDANT_ARCH_OVERRIDES_LINT)
							.snippet(Snippet::new_variable(
								sess, apml, var_name,
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

				// arch-groups
				for (var_name, base_name, group, is_array) in arch_groups {
					if !ACBS_VARIABLES.contains(&base_name.as_str()) {
						continue;
					}

					if let Some(targets) =
						sess.ab4_data.arch_groups.get(group.as_str())
					{
						for target in targets {
							let (okay, fixable) =
								if let Some((_, groups)) = arch_overrides.get(
									&(base_name.clone(), target.to_string()),
								) {
									(groups.contains(&group), false)
								} else {
									(false, true)
								};

							if !okay {
								LintMessage::new(ACBS_ARCH_GROUPS_LINT)
									.message(format!(
										"'{var_name}' is not included in target '{target}'",
									))
									.snippet(Snippet::new_variable(
										sess, apml, &var_name,
									))
									.emit(sess);
								if !sess.dry && fixable {
									apml.with_editor(|editor| {
										let name =format!(
												"{}__{}",
												base_name,
												target.to_ascii_uppercase()
											);
										let value = if is_array {
											ast::VariableValue::Array(vec![ast::ArrayElement::ArrayInclusion(var_name.to_string().into())])
										} else {
											ast::VariableValue::String(ast::Text(vec![ast::Word::Variable(ast::VariableExpansion{ name: var_name.to_string().into(), modifier: None })]))
										};
										editor.append_var_ast(
											name,
											&value,
											Some(&var_name),
										);
									});
								}
							}
						}
					}
				}

				Ok::<_, anyhow::Error>(())
			})?;
		}
		Ok(())
	}
}
