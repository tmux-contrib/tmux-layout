mod app;
mod layout;
mod log;

use std::io::IsTerminal;
use std::process::ExitCode;

use crate::app::args::*;
use crate::app::exec::*;
use crate::layout::*;

use anyhow::{Context, Result};
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
        ProgramCommand::New(args) => {
            let cwd = std::env::current_dir().context("failed to read the current directory")?;
            let environment = environment();
            let mut command = NewCommand { cwd, environment };
            command.execute(&args)
        }
        ProgramCommand::Edit(args) => {
            let environment = environment();
            let editor = ["VISUAL", "EDITOR"]
                .into_iter()
                .find_map(|name| var(&environment, name))
                .unwrap_or("vi")
                .to_string();
            // Only offer to re-open the editor to someone at a terminal
            let interactive = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
            let confirm = interactive.then(|| Box::new(log::confirm) as Box<_>);
            let mut command = EditCommand {
                editor,
                environment,
                confirm,
            };
            command.execute(&args)
        }
        ProgramCommand::Save(args) => {
            let tmux = Box::new(Client::new());
            let environment = environment();
            let mut command = SaveCommand { tmux, environment };
            command.execute(&args)
        }
        ProgramCommand::List(args) => {
            let writer = Box::new(std::io::stdout());
            let table = std::io::stdout().is_terminal();
            let width = console::Term::stdout()
                .size_checked()
                .map(|(_, width)| width.into());
            let tmux = Box::new(Client::new());
            let environment = environment();
            let mut command = ListCommand {
                writer,
                table,
                width,
                tmux,
                environment,
            };
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
