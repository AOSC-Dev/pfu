//! libpfu (PackFixerUp) is a library for linting and fixing AOSC OS
//! package build script automatically.

use async_trait::async_trait;

pub mod context;
pub use context::Context;

/// A kind of fix or lint.
#[async_trait]
pub trait Fixer {
    async fn apply(&self, ctx: &Context);
}
