use std::{collections::HashSet, io::Write};

use anyhow::Result;
use console::style;
use kstring::KString;
use libpfu::message::LintMessage;
use log::{Level, LevelFilter, Metadata, Record};

struct Logger(Level);

impl log::Log for Logger {
	fn enabled(&self, metadata: &Metadata) -> bool {
		metadata.level() <= self.0
	}

	fn log(&self, record: &Record) {
		if self.enabled(record.metadata()) {
			match record.level() {
				Level::Error => {
					eprintln!(
						"{}{}",
						style("error: ").red().bold(),
						record.args()
					);
				}
				Level::Warn => {
					eprintln!(
						"{}{}",
						style("warn:  ").yellow().bold(),
						record.args()
					);
				}
				Level::Info => {
					eprintln!(
						"{}{}",
						style("info:  ").cyan().bold(),
						record.args()
					);
				}
				Level::Debug => {
					eprintln!(
						"{}{}",
						style("debug: ").dim().bold(),
						record.args()
					);
				}
				Level::Trace => unreachable!(),
			}
		}
	}

	fn flush(&self) {}
}

pub fn init(debug: bool) -> Result<()> {
	log::set_boxed_logger(Box::new(Logger(if debug {
		log::Level::Debug
	} else {
		log::Level::Info
	})))
	.map(|_| {
		log::set_max_level(if debug {
			LevelFilter::Debug
		} else {
			LevelFilter::Info
		})
	})?;
	Ok(())
}

pub struct LintReporter {
	pub disabled_lints: HashSet<KString>,
}

impl LintReporter {
	/// Prints a lint message to stderr.
	pub fn report(
		&self,
		message: LintMessage,
		mut to: impl Write,
	) -> Result<()> {
		if self.disabled_lints.contains(message.lint.ident) {
			return Ok(());
		}
		let level = match message.lint.level {
			libpfu::Level::Note => style("note:  ").dim().bold(),
			libpfu::Level::Info => style("info:  ").cyan().bold(),
			libpfu::Level::Warning => style("warn:  ").yellow().bold(),
			libpfu::Level::Error => style("error: ").red().bold(),
		};
		writeln!(to, "{}{}", level, style(message.message).bold())?;
		for note in message.notes {
			writeln!(
				to,
				"       {}{}",
				style("note: ").dim().bold(),
				style(note).dim()
			)?;
		}
		for snippet in message.snippets {
			write!(to, "       {}{}", style("--> ").blue(), snippet.path)?;
			if let Some(line) = snippet.line {
				write!(to, ":{line}")?;
			}
			if let Some(source) = snippet.source {
				write!(to, ": {source}")?;
			}
			writeln!(to)?;
		}
		Ok(())
	}
}
