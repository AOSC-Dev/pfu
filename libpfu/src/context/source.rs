//! Source-code access layers.

use anyhow::Result;
use libabbs::apml::{value::array::StringArray, ApmlContext};
use opendal::Operator;

use super::Context;

/// Initializes the source code access for a context.
pub async fn open(ctx: &Context) -> Result<Operator> {
    let spec_src = String::from_utf8(ctx.fs.read("spec").await?.to_vec())?;
    let spec_ctx = ApmlContext::eval_source(&spec_src)?;
    let srcs_str = spec_ctx.read("SRCS").into_string();
    let srcs = StringArray::from(srcs_str);
    todo!()
}
