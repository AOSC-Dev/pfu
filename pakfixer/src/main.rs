use std::{path::PathBuf, time::SystemTime};

use anyhow::{Result, bail};
use clap::Parser;
use console::style;
use libabbs::tree::AbbsTree;
use libpfu::Session;
use log::{debug, error, info};
use logger::LintReporter;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use regex::Regex;
use selector::LinterSelector;

pub mod linters;
pub mod logger;
pub mod selector;

#[derive(Parser, Debug)]
#[command(
	version,
	about = "PackFixerUpper: bring up AOSC OS packages magically"
)]
struct Args {
	/// Path of ABBS tree.
	#[arg(short = 'C', env = "ABBS_TREE")]
	tree: Option<PathBuf>,
	/// Package name.
	#[arg(required_unless_present_any = ["section", "regex", "world"])]
	name: Option<String>,
	/// Process all packages in a section.
	#[arg(short, long)]
	section: Option<String>,
	/// Process all packages matching the given regex.
	#[arg(short, long)]
	regex: Option<Regex>,
	/// Process all packages in the tree.
	#[arg(long)]
	world: bool,
	/// Dry run.
	#[arg(short, long)]
	dry: bool,
	/// Run without network.
	#[arg(long, env = "NO_NETWORK")]
	offline: bool,
	/// Linter selector directives.
	#[arg(short = 'W')]
	directives: Vec<String>,
	/// Enable more logging.
	#[cfg(debug_assertions)]
	#[arg(long)]
	debug: bool,
	/// Enable less logging.
	#[arg(short, long)]
	quiet: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
	let args = Args::parse();
	#[cfg(debug_assertions)]
	logger::init(args.debug)?;
	#[cfg(not(debug_assertions))]
	logger::init(false)?;

	let abbs = AbbsTree::new(
		args.tree
			.unwrap_or_else(|| std::env::current_dir().unwrap()),
	);

	info!("PackFixerUpper {}", env!("CARGO_PKG_VERSION"));

	let packages = if let Some(name) = args.name {
		vec![abbs.find_package(name)?]
	} else if let Some(section) = args.section {
		abbs.section_packages(&section.into())?
	} else if let Some(regex) = args.regex {
		abbs.all_packages()?
			.into_par_iter()
			.filter(|pkg| regex.is_match(pkg.name()))
			.collect()
	} else if args.world {
		abbs.all_packages()?
	} else {
		bail!("Package name must be specified")
	};

	let mut linters = LinterSelector::default();
	for directive in args.directives {
		linters.apply(&directive);
	}
	let (linters, disabled_lints) = linters.select();
	let reporter = LintReporter { disabled_lints };
	let linters = linters
		.iter()
		.map(|linter| (linter.ident, linter.create()))
		.collect::<Vec<_>>();

	let total_packages = packages.len();
	let total_linters = linters.len();
	info!(
		"Selected {} packages, {} linters",
		total_packages, total_linters
	);

	let start_time = SystemTime::now();
	for (index, package) in packages.into_iter().enumerate() {
		if !args.quiet {
			eprintln!(
				"{} [{}/{}] {}/{}",
				style("    Checking").green().bold(),
				index + 1,
				total_packages,
				package.section(),
				package.name()
			);
		}
		let name = package.name().to_string();
		let mut sess = match Session::new(abbs.clone(), package) {
			Ok(sess) => sess,
			Err(err) => {
				error!(
					"Session initialization failed for {}: {:#?}",
					name, err
				);
				continue;
			}
		};
		sess.dry = args.dry;
		sess.offline = args.offline;
		for (ident, linter) in &linters {
			match linter.apply(&sess).await {
				Ok(_) => {
					debug!("{} finished on {}", ident, name);
				}
				Err(err) => {
					error!("{} failed on {}: {:#?}", ident, name, err);
					continue;
				}
			};
			let mut stdout = std::io::stdout().lock();
			for message in sess.take_messages() {
				reporter.report(message, &mut stdout)?;
			}
		}
	}

	let elapsed = start_time.elapsed()?;
	eprintln!(
		"{} {} packages, {} linters in {}s",
		style("    Finished").green().bold(),
		total_packages,
		total_linters,
		elapsed.as_secs(),
	);

	Ok(())
}
