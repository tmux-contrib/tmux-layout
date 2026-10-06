mod app;
mod layout;
mod log;

use std::process::ExitCode;

use crate::app::args::*;
use crate::app::exec::*;
use crate::layout::*;

use anyhow::Result;
use clap::Parser;

fn main() -> ExitCode {
    let program = Program::parse();
    let parent = program.command.parent();
    log::set_level(match (parent.quiet, parent.verbose) {
        (true, _) => log::Level::Quiet,
        (_, true) => log::Level::Verbose,
        _ => log::Level::Normal,
    });
    log::debug(format!("layout directory: {}", parent.layout_dir.display()));

    // Process the correct command
    match run(program) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            log::error(format!("{err:#}"));
            ExitCode::FAILURE
        }
    }
}

fn run(program: Program) -> Result<()> {
    match program.command {
        ProgramCommand::Switch(args) => {
            let tmux = Box::new(Client::new());
            let environment = environment();
            let mut command = SwitchCommand { tmux, environment };
            command.execute(&args)
        }
        ProgramCommand::List(args) => {
            let writer = Box::new(std::io::stdout());
            let mut command = ListCommand { writer };
            command.execute(&args)
        }
    }
}

/// Returns the environment variables of this process, skipping those that aren't Unicode.
fn environment() -> Environment {
    std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .collect()
}
