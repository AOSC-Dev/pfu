//! Context
//!
//! To apply a lint or fix to a package, callers must prepare a [Context],
//! providing enough information to fixers.

use std::{cell::OnceCell, path::PathBuf, sync::{Arc, RwLock}};

use anyhow::Result;

pub mod source;

/// A context including information related to the package to fix.
pub struct Context {
    /// VFS for access the ABBS tree.
    pub fs: opendal::Operator,
    /// Path to aosc-os-abbs tree.
    pub abbs_tree: PathBuf,
    /// Name of the package to check.
    pub package_name: String,
    /// Path of the package.
    pub package_path: PathBuf,
    /// Offline mode switch.
    pub offline: bool,

    /// Lazily initialized source
    source_storage: RwLock<OnceCell<Arc<opendal::Operator>>>,
}

impl Context {
    pub async fn source_fs(&self) -> Result<Arc<opendal::Operator>> {
        if let Some(result) = self.source_storage.read().unwrap().get() {
            Ok(result.clone())
        } else {
            let write = self.source_storage.write().unwrap();
            if let Some(result) = write.get() {
                Ok(result.clone())
            } else {
                write
                    .set(source::open(self).await?.into())
                    .expect("race condition");
                Ok(write.get().unwrap().clone())
            }
        }
    }
}
