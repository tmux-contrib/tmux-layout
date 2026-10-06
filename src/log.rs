use console::style;
use std::fmt::Display;
use std::sync::atomic::{AtomicU8, Ordering};

/// How much tmux-layout prints to stderr.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Warnings and errors only.
    Quiet = 0,
    /// Also informational messages.
    Normal = 1,
    /// Also details for debugging.
    Verbose = 2,
}

static LEVEL: AtomicU8 = AtomicU8::new(Level::Normal as u8);

/// Sets how much tmux-layout prints to stderr.
pub fn set_level(level: Level) {
    LEVEL.store(level as u8, Ordering::Relaxed);
}

/// Returns true if messages of `level` are printed.
fn enabled(level: Level) -> bool {
    LEVEL.load(Ordering::Relaxed) >= level as u8
}

/// Prints an error to stderr.
pub fn error(message: impl Display) {
    eprintln!(
        "tmux-layout: {} {message}",
        style("error:").red().bold().for_stderr()
    );
}

/// Prints an informational message to stderr, unless quiet.
pub fn info(message: impl Display) {
    if enabled(Level::Normal) {
        eprintln!("tmux-layout: {message}");
    }
}

/// Prints a detail for debugging to stderr, if verbose.
pub fn debug(message: impl Display) {
    if enabled(Level::Verbose) {
        eprintln!(
            "{}",
            style(format!("tmux-layout: debug: {message}"))
                .dim()
                .for_stderr()
        );
    }
}
