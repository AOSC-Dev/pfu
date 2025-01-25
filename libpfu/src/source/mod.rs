//! Source-code access layers.

use std::fs;

use anyhow::{Context, Result};
use libabbs::apml::{ApmlContext, value::array::StringArray};
use opendal::Operator;

use super::Session;

/// Initializes the source code access for a context.
pub async fn open(ctx: &Session) -> Result<Operator> {
	let spec_src = fs::read_to_string(ctx.package.join("spec"))
		.context("Cannot read spec file")?;
	let spec_ctx = ApmlContext::eval_source(&spec_src)?;
	let srcs_str = spec_ctx.read("SRCS").into_string();
	let _srcs = StringArray::from(srcs_str);
	todo!()
}
