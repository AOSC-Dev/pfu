//! Spaces and newline checks.

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
	pub EXTRA_SPACES_LINTER,
	ExtraSpacesLinter,
	["extra-spaces"]
}

declare_lint! {
	pub EXTRA_SPACES_LINT,
	"extra-spaces",
	Warning,
	"extra spaces should be removed"
}

#[async_trait]
impl Linter for ExtraSpacesLinter {
	async fn apply(&self, sess: &Session) -> Result<()> {
		for mut apml in walk_apml(sess) {
			debug!("Looking for extra spaces in {apml:?}");
			let mut ranges = apml
				.lst()
				.0
				.iter()
				.enumerate()
				.batching(|iter| {
					let mut ret = Some(
						iter.take_while(|(_, token)| {
							!matches!(token, lst::Token::Newline)
						})
						.collect_vec(),
					);
					// discard newline
					_ = iter.next();
					ret.take_if(|tokens| !tokens.is_empty())
				})
				.filter(|line| {
					matches!(line.first(), Some((_, lst::Token::Spacy(_))))
						|| matches!(
							line.last(),
							Some((_, lst::Token::Spacy(_)))
						)
				})
				.inspect(|line| {
					let index = line[0].0;
					LintMessage::new(EXTRA_SPACES_LINT)
						.snippet(Snippet::new_index(sess, &apml, index))
						.emit(sess);
				})
				.map(|line| {
					let mut before = 0;
					while let Some((_, lst::Token::Spacy(_))) = line.get(before)
					{
						before += 1;
					}
					let mut after = 0;
					while let Some((_, lst::Token::Spacy(_))) =
						line.get(line.len() - after)
					{
						after += 1;
					}
					let first_idx = line.first().unwrap().0;
					let last_idx = line.last().unwrap().0;
					(first_idx..first_idx + before, last_idx - after..last_idx)
				})
				.collect_vec();
			debug!(
				"Found {} lines with extra spaces in {:?}",
				ranges.len(),
				apml
			);
			if !sess.dry && !ranges.is_empty() {
				// ranges must be reversed to avoid removing earlier ranges
				// from invalidating later ranger
				ranges.reverse();
				apml.with_upgraded(|apml| {
					apml.with_lst(|lst| {
						for (range1, range2) in ranges {
							lst.0.drain(range2);
							lst.0.drain(range1);
						}
					});
				});
			}
		}
		Ok(())
	}
}
