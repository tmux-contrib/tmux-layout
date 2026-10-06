//! Layouts: where they are stored, how they are read, and the tmux they are applied to.

mod config;
mod shell;
mod tmux;

pub use config::*;
pub use shell::*;
pub use tmux::*;
