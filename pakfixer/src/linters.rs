//! List of known linters.

use libpfu::LinterMetadata;
use libpfu_fixers::{
	fish_shell::FISH_SHELL_LINTER,
	python::{deps::PYTHON_DEPS_LINTER, pep517::PEP517_LINTER},
};
use libpfu_style::{
	archgroup::ARCH_GROUP_LINTER, chkupd::CHKUPDATE_LINTER, empty_line::EMPTY_LINE_LINTER, sources::SRCS_LINTER, spacing::EXTRA_SPACES_LINTER
};

pub type LinterPreset = &'static [&'static LinterMetadata];

/// Linter presets index.
///
/// - `full`: All known linters
/// - `baseline`: The default linter set.
/// - `extra`: Extra linters.
/// - `pedantic`: Even more linters.
/// - `crazy`: Linters that may make situation worse.
pub static LINTER_PRESETS: &[(&str, LinterPreset)] = &[
	("full", FULL_LINTERS),
	("baseline", BASELINE_LINTERS),
	("extra", EXTRA_LINTERS),
	("pedantic", PEDANTIC_LINTERS),
	("crazy", CRAZY_LINTERS),
];

pub static FULL_LINTERS: LinterPreset = &[
	EXTRA_SPACES_LINTER,
	EMPTY_LINE_LINTER,
	SRCS_LINTER,
	CHKUPDATE_LINTER,
	FISH_SHELL_LINTER,
	PEP517_LINTER,
	PYTHON_DEPS_LINTER,
	ARCH_GROUP_LINTER,
];
pub static BASELINE_LINTERS: LinterPreset = &[
	EXTRA_SPACES_LINTER,
	EMPTY_LINE_LINTER,
	SRCS_LINTER,
	CHKUPDATE_LINTER,
	FISH_SHELL_LINTER,
	ARCH_GROUP_LINTER,
];
pub static EXTRA_LINTERS: LinterPreset = &[PEP517_LINTER, PYTHON_DEPS_LINTER];
pub static PEDANTIC_LINTERS: LinterPreset = &[];
pub static CRAZY_LINTERS: LinterPreset = &[];

pub fn find(name: &str) -> Option<&'static LinterMetadata> {
	FULL_LINTERS
		.iter()
		.find(|linter| linter.ident == name)
		.copied()
}
