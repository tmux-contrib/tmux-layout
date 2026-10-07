use anyhow::Result;
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

/// Prints the outcome of an action to stderr, unless quiet.
pub fn success(message: impl Display) {
    if enabled(Level::Normal) {
        eprintln!("{} {message}", style("✓").green().bold().for_stderr());
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

/// Asks `question` on stderr and reads the answer from stdin; yes unless it is `n` or `no`.
/// End of input answers no.
pub fn confirm(question: &str) -> Result<bool> {
    eprint!(
        "{} {question} {} ",
        style("?").cyan().bold().for_stderr(),
        style("[Y/n]").dim().for_stderr()
    );
    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer)? == 0 {
        eprintln!();
        return Ok(false);
    }
    Ok(!matches!(answer.trim().to_lowercase().as_str(), "n" | "no"))
}

/// Returns `count` with `noun`, pluralized by appending `s` (e.g. "1 window", "3 windows").
pub fn count(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_pluralizes() {
        assert_eq!(count(0, "window"), "0 windows");
        assert_eq!(count(1, "window"), "1 window");
        assert_eq!(count(3, "window"), "3 windows");
    }
}
