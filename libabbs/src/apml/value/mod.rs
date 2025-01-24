//! Wrappers for common metadata formats.
//!
//! Currently the following formats are supported:
//! - [StringArray][array::StringArray]: `xxx yyy zzz`
//! - [Union][union::Union]: `git::commit=xxx::schema://xxx`

pub mod array;
pub mod union;
