use std::{env, fs, collections::BTreeMap, path::{Path, PathBuf}};

use attaca::{Init, Open, Store, digest::prelude::*, store::prelude::*};
use failure::*;
use futures::prelude::*;
use leveldb::{database::Database, kv::KV, options::{Options, ReadOptions}};
use url::{self, Url};

use Repository;
use config::{Config, StoreConfig, StoreKind};
use db::Key;
use init::{InitArgs, InitStore};
use plumbing;
use syntax::{Name, Ref};

/// Create a local repository by cloning data from a remote repository. The default store type is `leveldb`.
#[derive(Debug, Clone, StructOpt, Builder)]
#[structopt(name = "clone")]
pub struct CloneArgs {
    /// URL of the repository to clone.
    #[structopt(name = "URL", parse(try_from_str = "Url::parse"))]
    url: Url,

    /// Path to a directory to initialize as a repository. This defaults to the current directory.
    #[structopt(name = "PATH", parse(from_os_str))]
    path: Option<PathBuf>,

    #[structopt(subcommand)]
    store: Option<InitStore>,
}

pub struct CloneOut {
    pub blocking: Box<Future<Item = (), Error = Error>>,
}

#[macro_export]
macro_rules! clone {
    (@inner $args:expr, $repo:ident, $generic:expr, $($lcname:ident, $ccname:ident : $type:ty),*) => {
        {
            let args: CloneArgs = $args;
            let init_args = InitArgs {
                path: args.path,
                store: args.store,
            };

            match () {
                $(_ if <$type as $crate::reexports::attaca::Open>::SCHEMES
                    .contains(args.url.scheme()) => {
                    init!(init_args, repository, {
                        let head;
                        let candidate;

                        let repository = {
                            use $crate::reexports::futures::prelude::*;
                            let store =
                                Store::new(<$type as $crate::reexports::attaca::Open>
                                    ::open(args.url.as_str())?);

                            let branches = store.load_branches().wait()?;
                            let local_master =
                                store::copy(branches["master"].clone(), repository.store.clone()).wait()?;
                            let head = CommitRef::new(local_master);
                            let candidate = head.fetch().wait()?.as_subtree().clone();
                        };

                        {
                            #[allow(unused_mut)]
                            let mut $repo = repository;
                            Ok({
                                #[warn(unused_mut)]
                                $generic
                            })
                        }
                    })
                })*
                _ => unreachable!(),
            }
        }
    };
    ($args:expr, $repo:ident, $generic:expr) => {
        all_backends!(clone!(@inner $args, $repo, $generic))
    };
}

pub fn clone(args: CloneArgs) -> CloneOut {
    let init_args = InitArgs {
        path: args.path,
        store: args.store,
    };
    let url = args.url;
    let blocking = init!(init_args, repository, clone_from(repository, url));

    CloneOut {
        blocking: Box::new(blocking.into_future().flatten()),
    }
}

fn clone_from<B: Backend>(
    mut this: Repository<B>,
    url: Url,
) -> Box<Future<Item = (), Error = Error>> {
    let blocking = async_block! {
        let origin = "origin".parse::<Name>()?;
        let master = "master".parse::<Name>()?;
        // NB wait here because of issues w/ borrowing in generators.
        plumbing::remote::add(&mut this, origin.clone(), url).wait()?;
        plumbing::fetch::remote(&mut this, origin.clone()).wait()?;
        plumbing::checkout::by_ref(&mut this, Ref::Remote(origin, master)).wait()?;
        Ok(())
    };

    Box::new(blocking)
}
