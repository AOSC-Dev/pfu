use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    version,
    about,
    long_about = "PackFixerUpper: bring up AOSC OS packages magically"
)]
struct Args {
    /// Path of ABBS tree.
    #[arg(short = 'C', env = "ABBS_TREE")]
    tree: Option<PathBuf>,

    /// Package name.
    #[arg()]
    name: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if let Some(tree) = args.tree {
        std::env::set_current_dir(tree)?;
    }

    Ok(())
}
