//! Layouts: where they are stored, how they are read and captured, and the tmux they are
//! applied to.

mod capture;
mod config;
mod shell;
mod tmux;

pub use capture::*;
pub use config::*;
pub use shell::*;
pub use tmux::*;
