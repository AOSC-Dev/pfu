//! Lint messages.

use std::borrow::Cow;

use libabbs::apml::lst;

use crate::{LintMetadata, Session, apml::ApmlFileAccess};

/// A lint message produced by linters.
#[derive(Debug)]
pub struct LintMessage {
	pub lint: &'static LintMetadata,
	pub message: Cow<'static, str>,
	pub notes: Vec<String>,
	pub snippets: Vec<Snippet>,
}

impl LintMessage {
	/// Creates a new lint message.
	pub fn new(lint: &'static LintMetadata) -> Self {
		Self {
			lint,
			message: Cow::Borrowed(lint.desc),
			snippets: Vec::new(),
			notes: Vec::new(),
		}
	}

	/// Adds this message to the outbox to the given session.
	pub fn emit(self, sess: &Session) {
		sess.outbox.lock().push(self);
	}

	/// Sets a non-default message.
	pub fn message(mut self, message: String) -> Self {
		self.message = message.into();
		self
	}

	/// Appends a note.
	pub fn note(mut self, note: String) -> Self {
		self.notes.push(note);
		self
	}

	/// Appends a snippet.
	pub fn snippet(mut self, snippet: Snippet) -> Self {
		self.snippets.push(snippet);
		self
	}
}

/// A snippet of code to annotate.
#[derive(Debug)]
pub struct Snippet {
	pub path: String,
	pub line: Option<usize>,
	pub source: Option<String>,
}

impl Snippet {
	pub fn new_token(
		sess: &Session,
		apml: &ApmlFileAccess,
		token: &lst::Token<'_>,
	) -> Self {
		let lst = apml.lst();
		let path = apml
			.path()
			.strip_prefix(sess.tree.as_path())
			.unwrap_or(apml.path())
			.to_string_lossy()
			.to_string();
		let line = lst.0.iter().position(|t| t == token).map(|index| {
			lst.0[0..index]
				.iter()
				.filter(|token| matches!(token, lst::Token::Newline))
				.count() + 1
		});
		let source = match token {
			lst::Token::Spacy(_) | lst::Token::Newline => None,
			lst::Token::Comment(_) | lst::Token::Variable(_) => {
				Some(token.to_string())
			}
		};
		Self { path, line, source }
	}

	pub fn new(
		sess: &Session,
		apml: &ApmlFileAccess,
		token: usize,
	) -> Self {
		let lst = apml.lst();
		let path = apml
			.path()
			.strip_prefix(sess.tree.as_path())
			.unwrap_or(apml.path())
			.to_string_lossy()
			.to_string();
		let line = lst.0[0..token]
			.iter()
			.filter(|token| matches!(token, lst::Token::Newline))
			.count() + 1;
		Self {
			path,
			line: Some(line),
			source: None,
		}
	}
}
