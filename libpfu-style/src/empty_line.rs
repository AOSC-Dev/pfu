//! Empty-line checks.

use anyhow::Result;
use async_trait::async_trait;
use itertools::Itertools;
use libabbs::apml::lst;
use libpfu::{
	Linter, Session, declare_lint, declare_linter,
	message::{LintMessage, Snippet},
	walk_apml,
};
use log::debug;

declare_linter! {
	pub EMPTY_LINE_LINTER,
	EmptyLineLinter,
	[
		"missing-trailing-line",
		"too-many-trailing-empty-lines",
		"too-many-empty-lines",
	]
}

declare_lint! {
	pub MISSING_TRAILING_LINE_LINT,
	"missing-trailing-line",
	Warning,
	"missing empty line at the end"
}

declare_lint! {
	pub TOO_MANY_TRAILING_EMPTY_LINES,
	"too-many-trailing-empty-lines",
	Warning,
	"too many trailing empty lines"
}

declare_lint! {
	pub TOO_MANY_EMPTY_LINES,
	"too-many-empty-lines",
	Warning,
	"more than two empty lines"
}

#[async_trait]
impl Linter for EmptyLineLinter {
	async fn apply(&self, sess: &Session) -> Result<()> {
		for mut apml in walk_apml(sess) {
			{
				debug!("Looking for missing trailing new lines in {:?}", apml);
				let missing_new_line = apml
					.lst()
					.0
					.iter()
					.rev()
					.take_while(|token| !matches!(token, lst::Token::Newline))
					.any(|token| token.is_empty());
				if missing_new_line {
					LintMessage::new(MISSING_TRAILING_LINE_LINT)
						.snippet(Snippet::new_index(
							sess,
							&apml,
							apml.lst().0.len() - 1,
						))
						.emit(sess);
					if !sess.dry {
						apml.with_upgraded(|apml| {
							apml.with_lst(|lst| lst.0.push(lst::Token::Newline))
						});
					}
				}
			}
			{
				debug!("Counting trailing empty lines in {:?}", apml);
				let trailing_newlines = apml
					.lst()
					.0
					.iter()
					.enumerate()
					.rev()
					.take_while(|(_, token)| token.is_empty())
					.filter(|(_, token)| matches!(token, lst::Token::Newline))
					.collect_vec();
				if trailing_newlines.len() > 1 {
					LintMessage::new(TOO_MANY_TRAILING_EMPTY_LINES)
						.snippet(Snippet::new_index(
							sess,
							&apml,
							apml.lst().0.len() - 1,
						))
						.emit(sess);
					if !sess.dry {
						let start = trailing_newlines.first().unwrap().0 + 1;
						apml.with_upgraded(|apml| {
							apml.with_lst(|lst| lst.0.truncate(start - 1))
						});
					}
				}
			}
			{
				debug!("Counting continuous empty lines in {:?}", apml);
				enum State {
					NotEmpty,
					Empty { from: usize, lines: usize },
				}
				let mut state = State::NotEmpty;
				let mut ranges = Vec::new();
				for (idx, token) in apml.lst().0.iter().enumerate() {
					if token.is_empty() {
						match state {
							State::NotEmpty => {
								state = State::Empty {
									from: idx,
									lines: 0,
								}
							}
							State::Empty { from: _, lines: _ } => {}
						}
						if matches!(token, lst::Token::Newline) {
							if let State::Empty { from: _, lines } = &mut state
							{
								*lines += 1;
							} else {
								unreachable!()
							}
						}
					} else {
						match state {
							State::NotEmpty => {}
							State::Empty { from, lines } => {
								state = State::NotEmpty;
								if lines > 2 {
									LintMessage::new(TOO_MANY_EMPTY_LINES)
										.snippet(Snippet::new_index(
											sess, &apml, from,
										))
										.emit(sess);
									ranges.push(from..idx);
								}
							}
						}
					}
				}
				// newlines at the end of file is handled in previous check
				// so skipping them here
				if !sess.dry {
					ranges.reverse();
					if !ranges.is_empty() {
						apml.with_upgraded(|apml| {
							apml.with_lst(|lst| {
								for range in ranges {
									lst.0.drain(range.start..range.end);
									lst.0.insert(
										range.start,
										lst::Token::Newline,
									);
									lst.0.insert(
										range.start,
										lst::Token::Newline,
									);
								}
							})
						});
					}
				}
			}
		}
		Ok(())
	}
}
