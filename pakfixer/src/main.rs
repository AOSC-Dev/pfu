use std::{path::PathBuf, sync::Arc, time::SystemTime};

use anyhow::{Context, Result, bail};
use clap::Parser;
use console::style;
use libabbs::tree::AbbsTree;
use libpfu::{Session, absets::Autobuild4Data, walk_apml};
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
	name: Vec<String>,
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

	let packages = if !args.name.is_empty() {
		let mut packages = Vec::new();
		// TODO: replace with try_collect
		for name in args.name {
			packages.push(abbs.find_package(name)?);
		}
		packages
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
		"Selected {total_packages} packages, {total_linters} linters"
	);

	let ab4_data = Autobuild4Data::load_local()?.map(Arc::new);

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
		let mut sess =
			match Session::new(abbs.clone(), package.clone(), ab4_data.clone())
			{
				Ok(sess) => sess,
				Err(err) => {
					error!(
						"Session initialization failed for {:?}: {:#?}",
						&package, err
					);
					continue;
				}
			};
		sess.dry = args.dry;
		sess.offline = args.offline;
		for (ident, linter) in &linters {
			match linter.apply(&sess).await {
				Ok(_) => {
					debug!("{} finished on {:?}", ident, &package);
				}
				Err(err) => {
					error!("{} failed on {:?}: {:?}", ident, &package, err);
				}
			};
			let messages = sess.take_messages();
			if messages.is_empty() {
				continue;
			}
			let mut stdout = std::io::stdout().lock();
			for message in messages {
				#[cfg(debug_assertions)]
				if !linter.metadata().lints.contains(&message.lint.ident) {
					bail!(
						"Linter {} emitted a lint message of {} which is not included in its linter metadata",
						ident,
						message.lint.ident
					);
				}
				reporter.report(message, &mut stdout)?;
			}
		}
		if !sess.dry {
			debug!("Saving APML files for {:?}", &package);
			for mut apml in walk_apml(&sess) {
				if apml.is_dirty() {
					apml.with_upgraded(|apml| apml.save())
						.with_context(|| format!("saving {apml:?}"))?;
				}
			}
		} else {
			#[cfg(debug_assertions)]
			{
				debug!("Checking APML files sync states for {:?}", &package);
				for apml in walk_apml(&sess) {
					if apml.is_dirty() {
						bail!("APML file is desynced in dry-run session");
					}
				}
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
