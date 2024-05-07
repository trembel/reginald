use std::io;

use crate::error::Error;
use clap::{CommandFactory, Parser};

use crate::cli::Cli;

#[derive(Parser, Debug)]
#[command(about = "Print completion script for specified shell")]
pub struct Command {
    pub shell: clap_complete::Shell,
}

pub fn cmd(compl: Command) -> Result<(), Error> {
    clap_complete::generate(compl.shell, &mut Cli::command(), "reginald", &mut io::stdout());
    Ok(())
}
