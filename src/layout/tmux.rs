use anyhow::{bail, Context, Result};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

use crate::layout::quote;
use crate::log::debug;

/// Tmux runs tmux commands.
pub trait Tmux {
    /// Runs tmux with `args` and returns what it printed, trimmed. Fails when tmux does.
    fn run(&self, args: &[String]) -> Result<String>;
    /// Replaces this process with tmux running `args`, for commands that take over the
    /// terminal. Returns only when tmux can't be started.
    fn exec(&self, args: &[String]) -> Result<()>;
}

/// Client runs the `tmux` binary on the PATH.
#[derive(Default)]
pub struct Client;

impl Client {
    /// Creates a new client.
    pub fn new() -> Self {
        Self
    }

    /// Returns a `tmux` command with `args`.
    fn command(args: &[String]) -> Command {
        debug(format!(
            "tmux {}",
            args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ")
        ));
        let mut command = Command::new("tmux");
        command.args(args);
        command
    }
}

impl Tmux for Client {
    fn run(&self, args: &[String]) -> Result<String> {
        let output = Self::command(args)
            .stdin(Stdio::null())
            .output()
            .context("failed to run tmux; is it installed?")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let name = args.first().map(String::as_str).unwrap_or_default();
            bail!("tmux {name} failed: {}", stderr.trim());
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn exec(&self, args: &[String]) -> Result<()> {
        let err = Self::command(args).exec();
        Err(err).context("failed to run tmux; is it installed?")
    }
}
