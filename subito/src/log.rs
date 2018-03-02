use std::{fmt, collections::HashSet};

use attaca::{HandleDigest, Store, digest::Digest,
             object::{Commit, CommitBuilder, CommitRef, TreeRef}};
use failure::*;
use futures::{stream, prelude::*};
use hex;

use Repository;
use quantified::{QuantifiedOutput, QuantifiedRef};

/// Show commit history sorted chronologically.
#[derive(Default, Debug, StructOpt, Builder)]
#[structopt(name = "log")]
pub struct LogArgs {}

impl<'r> QuantifiedOutput<'r> for LogArgs {
    type Output = LogOut<'r>;
}

impl QuantifiedRef for LogArgs {
    fn apply_ref<'r, S, D>(self, repository: &'r Repository<S, D>) -> Result<LogOut<'r>, Error>
    where
        S: Store,
        D: Digest,
        S::Handle: HandleDigest<D>,
    {
        Ok(repository.log(self))
    }
}

#[must_use = "LogOut contains futures which must be driven to completion!"]
pub struct LogOut<'r> {
    pub entries: Box<Stream<Item = (CommitRef<String>, Commit<String>), Error = Error> + 'r>,
}

impl<'r> fmt::Debug for LogOut<'r> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("LogOut")
            .field("entries", &"OPAQUE")
            .finish()
    }
}

impl<S: Store, D: Digest> Repository<S, D>
where
    S::Handle: HandleDigest<D>,
{
    pub fn log<'r>(&'r self, _args: LogArgs) -> LogOut<'r> {
        let entries = async_stream_block! {
            let state = self.get_state()?;

            let head = match state.head {
                Some(head) => head,
                None => return Ok(()),
            };

            let mut visited = HashSet::new();
            let mut queue = vec![head];

            while let Some(commit_ref) = queue.pop() {
                let commit = await!(commit_ref.fetch())?;
                queue.extend(commit.as_parents().iter().filter_map(|parent| {
                    if visited.insert(parent.clone()) {
                        Some(parent.clone())
                    } else {
                        None
                    }
                }));

                let mut builder = CommitBuilder::new();
                let parent_stream =
                    stream::futures_ordered(commit.as_parents().to_owned().into_iter().map(
                        |commit_ref| {
                            commit_ref.digest().map(|commit_digest| {
                                CommitRef::new(hex::encode(commit_digest.as_inner().as_bytes()))
                            })
                        },
                    ));
                let subtree_future = commit
                    .as_subtree()
                    .digest()
                    .map(|subtree_digest| TreeRef::new(hex::encode(subtree_digest.as_inner().as_bytes())));
                let digest_future = commit_ref
                    .digest()
                    .map(|commit_digest| CommitRef::new(hex::encode(commit_digest.as_inner().as_bytes())));

                let (digest, subtree, parents) = await!(
                    digest_future

                        .join3(subtree_future, parent_stream.collect())
                )?;
                builder.subtree(subtree);
                builder.parents(parents);
                builder.author(commit.as_author().clone());
                builder.timestamp(commit.as_timestamp().clone());

                if let Some(message) = commit.as_message() {
                    builder.message(message.to_owned());
                }

                stream_yield!((digest, builder.into_commit().unwrap()));
            }

            Ok(())
        };

        LogOut {
            entries: Box::new(entries),
        }
    }
}
