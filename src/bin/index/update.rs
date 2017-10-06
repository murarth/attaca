use std::env;

use clap::{App, SubCommand, ArgMatches};

use attaca::repository::Repository;

use errors::*;


pub fn command() -> App<'static, 'static> {
    SubCommand::with_name("update").about("Update the index.")
}


pub fn go(_matches: &ArgMatches) -> Result<()> {
    let mut repository = Repository::find(env::current_dir()?)?;
    repository.index.update()?;

    Ok(())
}