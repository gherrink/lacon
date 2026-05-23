//! lacon CLI entry point — clap derive surface, 6-subcommand dispatch.

mod cli;
mod commands;

use clap::Parser;
use cli::{Cli, CliCommand};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let exit_code = match cli.command {
        CliCommand::Run { rule, argv } => commands::run::execute(rule, argv)?,
        CliCommand::Validate { path } => commands::validate::execute(&path)?,
        CliCommand::Init { user, project } => commands::init::execute(user, project)?,
        CliCommand::Stats {
            project,
            since,
            rule,
            bytes,
            all,
        } => commands::stats::execute(project, since, rule, bytes, all)?,
        CliCommand::Explain { id } => commands::explain::execute(id)?,
        CliCommand::Doctor => commands::doctor::execute()?,
    };
    std::process::exit(exit_code);
}
