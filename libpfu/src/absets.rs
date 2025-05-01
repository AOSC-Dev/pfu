use std::{
	collections::{HashMap, HashSet},
	fs,
};

use anyhow::Result;
use kstring::KString;

/// Static data of Autobuild4, from /usr/lib/autobuild4/sets.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Autobuild4Data {
	pub arch_groups: HashMap<KString, HashSet<KString>>,
}

impl Autobuild4Data {
	/// Loads Autobuild4 data from system.
	pub fn load_local() -> Result<Self> {
		let arch_groups = serde_json::from_str(&fs::read_to_string(
			"/usr/lib/autobuild4/sets/arch_groups.json",
		)?)?;
		Ok(Self { arch_groups })
	}
}
