//! lacon CLI entry point — clap derive surface, 6-subcommand dispatch.

mod cli;
mod commands;

use cli::{Cli, CliCommand};
use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let exit_code = match cli.command {
        CliCommand::Run { rule, argv } => commands::run::execute(rule, argv)?,
        CliCommand::Validate { path } => commands::validate::execute(&path)?,
        CliCommand::Init => commands::init::execute()?,
        CliCommand::Stats { project, since, rule } => {
            commands::stats::execute(project, since, rule)?
        }
        CliCommand::Explain { .. } => commands::explain::execute()?,
        CliCommand::Doctor => commands::doctor::execute()?,
    };
    std::process::exit(exit_code);
}
