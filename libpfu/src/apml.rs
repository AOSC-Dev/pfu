//! APML lifetime-ereased accessor wrappers.
//!
//! [ApmlFileAccess] provides a lifetime-free multi-level APML file accessing interface.
//!
//! To create such an accessor, you will need to call [ApmlFileAccess::open].
//!
//! The accessor parses and evaluates the APML file immediately after open.
//! Then, [lst][ApmlFileAccess::lst], [ast][ApmlFileAccess::ast] and
//! [ctx][ApmlFileAccess::ctx] can be called to retrieve read access to
//! [ApmlLst], [ApmlAst] and [ApmlContext].
//!
//! [with_lst][ApmlFileAccess::with_lst] can be used to modify the LST.
//! [with_editor][ApmlFileAccess::with_editor] wraps the LST with [ApmlEditor]
//! to make editing easier.
//!
//! During a modification transaction, all reading access will be blocked and
//! lead to a panic. Callers must ensure that no read access to AST or context
//! can be performed during a write.
//!
//! Accessors tracks the dirty state internally. Once the underlying LST
//! is modified, the dirty flag, which can be read with [dirty][ApmlFileAccess::dirty],
//! will be set to true, indicating that the caller (user) needs to call
//! [write][ApmlFileAccess::write] to save changes to disk (or by themselves).
//!
//! Accessors also guarantee that modifications to LST will be immediately
//! reflected on AST and context. This is implemented by clearing the AST
//! and context cache and re-emitting or re-evaluating them on the next access.

use std::{
	fmt::Debug,
	fs,
	path::{Path, PathBuf},
};

use anyhow::Result;
use libabbs::apml::{
	ApmlContext,
	ast::{ApmlAst, AstNode},
	editor::ApmlEditor,
	lst::ApmlLst,
};

/// Accessor wrapper for analyzing and modifying APML files.
pub struct ApmlFileAccess {
	/// Path to the APML files.
	path: PathBuf,
	/// Evaluate APML context.
	ctx: Option<ApmlContext>,
	/// Inner self-referencing wrapper.
	inner: ApmlFileAccessInner,
	/// Dirty mark.
	dirty: bool,
}

#[ouroboros::self_referencing]
struct ApmlFileAccessInner {
	/// Original file value.
	orig_text: String,
	/// APML lossless syntax tree.
	#[borrows(orig_text)]
	#[covariant]
	lst: Option<ApmlLst<'this>>,
	/// APML abstract syntax tree.
	#[borrows(orig_text)]
	#[covariant]
	ast: Option<ApmlAst<'this>>,
}

impl ApmlFileAccess {
	/// Opens a APML file accessor.
	pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
		let path = path.as_ref().to_owned();
		let text = fs::read_to_string(&path)?;
		// construct inner LST
		let mut inner = ApmlFileAccessInner::try_new(
			text,
			|text| Ok::<_, anyhow::Error>(Some(ApmlLst::parse(text.as_str())?)),
			|_| Ok(None),
		)?;
		// construct inner AST
		inner.with_mut(|inner| {
			let lst = inner.lst.as_ref().unwrap();
			let ast = ApmlAst::emit_from(lst)?;
			*inner.ast = Some(ast);
			Ok::<_, anyhow::Error>(())
		})?;
		// construct context
		let ctx = ApmlContext::eval_ast(inner.borrow_ast().as_ref().unwrap())?;
		Ok(Self {
			path,
			ctx: Some(ctx),
			inner,
			dirty: false,
		})
	}

	/// Returns the path to the APML file.
	pub fn path(&self) -> &Path {
		&self.path
	}

	/// Returns the dirty mark.
	pub fn dirty(&self) -> bool {
		self.dirty
	}

	/// Saves changes to disk and clears the dirty flag.
	pub fn write(&mut self) -> Result<()> {
		if self.dirty {
			self.dirty = false;
			let text = self.lst().to_string();
			fs::write(&self.path, text)?;
		}
		Ok(())
	}

	/// Gets a read reference to LST.
	#[must_use]
	pub fn lst(&self) -> &ApmlLst<'_> {
		self.inner
			.borrow_lst()
			.as_ref()
			.expect("APML LST has been moved for editing")
	}

	/// Modifies LST.
	pub fn with_lst<F, T>(&mut self, f: F) -> T
	where
		F: FnOnce(&mut ApmlLst<'_>) -> T,
	{
		self.ctx = None;
		self.dirty = true;
		self.inner.with_mut(move |inner| {
			*inner.ast = None;
			// take out LST to block other method's re-caching
			let mut lst = inner
				.lst
				.take()
				.expect("APML LST has been moved for editing");
			let ret = f(&mut lst);
			*inner.lst = Some(lst);
			ret
		})
	}

	/// Modifies LST with LST editor.
	pub fn with_editor<F, T>(&mut self, f: F) -> T
	where
		F: FnOnce(&mut ApmlEditor<'_>) -> T,
	{
		self.ctx = None;
		self.dirty = true;
		self.inner.with_mut(move |inner| {
			*inner.ast = None;
			// take out LST to block other method's re-caching
			let lst = inner
				.lst
				.take()
				.expect("APML LST has been moved for editing");
			let mut editor = ApmlEditor::wrap(lst);
			let ret = f(&mut editor);
			*inner.lst = Some(editor.unwrap());
			ret
		})
	}

	/// Gets a read reference to AST.
	pub fn ast(&mut self) -> Result<&ApmlAst<'_>> {
		if self.inner.borrow_ast().is_none() {
			self.inner.with_mut(|inner| {
				let lst = inner
					.lst
					.as_ref()
					.expect("APML LST has been moved for editing");
				let ast = ApmlAst::emit_from(lst)?;
				*inner.ast = Some(ast);
				Ok::<_, anyhow::Error>(())
			})?;
		}
		Ok(self.inner.borrow_ast().as_ref().unwrap())
	}

	/// Gets a read reference to APML context.
	pub fn ctx(&mut self) -> Result<&ApmlContext> {
		if self.ctx.is_none() {
			let ctx = ApmlContext::eval_ast(self.ast()?)?;
			self.ctx = Some(ctx);
		}
		Ok(self.ctx.as_ref().unwrap())
	}
}

impl Debug for ApmlFileAccess {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("ApmlFileAccess")
			.field("path", &self.path)
			.field("dirty", &self.dirty)
			.finish()
	}
}

#[cfg(test)]
mod test {
	use super::ApmlFileAccess;

	#[test]
	fn test_access() {
		let mut access = ApmlFileAccess::open("testdata/example").unwrap();
		let _ = access.lst();
		let _ = access.ast();
		let _ = access.ctx();
	}
}
