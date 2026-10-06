use clap::{Args, Parser, Subcommand};
use std::env;
use std::path::PathBuf;

/// Examples shown at the end of each command's help.
const PROGRAM_HELP: &str = "Get started:
  tmux-layout new dev     # create ~/.config/tmux/layouts/dev.yml
  tmux-layout edit dev    # declare your windows and panes
  tmux-layout switch dev  # open them";
const SWITCH_EXAMPLES: &str = "Examples:
  tmux-layout switch dev           # ~/.config/tmux/layouts/dev.yml
  tmux-layout switch \"$PROJECT\"    # a layout named after a variable
  tmux-layout switch -v dev        # print the tmux commands it runs";
const NEW_EXAMPLES: &str = "Examples:
  tmux-layout new dev          # a layout opening in the current directory
  tmux-layout new dev --force  # start over";
const EDIT_EXAMPLES: &str = "Examples:
  tmux-layout edit dev                       # in $VISUAL or $EDITOR
  EDITOR=\"code --wait\" tmux-layout edit dev  # in VS Code";
const LIST_EXAMPLES: &str = "Examples:
  tmux-layout list                                 # one name per line
  tmux-layout switch \"$(tmux-layout list | fzf)\"  # pick one";

/// Program is the main entry point for the tmux-layout CLI.
#[derive(Debug, Parser)]
#[command(
    name = "tmux-layout",
    about = "Apply YAML-defined tmux layouts.",
    long_about = "Declare tmux sessions, windows and panes in YAML files, and open them with one command.",
    after_help = PROGRAM_HELP,
    arg_required_else_help = true,
    version
)]
pub struct Program {
    /// Command specifies the subcommand to execute.
    #[command(subcommand)]
    pub command: ProgramCommand,
}

/// ProgramArgs holds the shared global flags available to every subcommand.
#[derive(Debug, Args)]
pub struct ProgramArgs {
    /// Path to the directory holding the layout files.
    #[arg(
        help = "Layout directory path.",
        env = "TMUX_LAYOUT_DIR",
        default_value_os_t = default_layout_dir(),
        long,
        short = 'd'
    )]
    pub layout_dir: PathBuf,

    /// Print warnings and errors only.
    #[arg(
        help = "Print warnings and errors only.",
        long,
        short,
        conflicts_with = "verbose"
    )]
    pub quiet: bool,

    /// Print details for debugging.
    #[arg(
        help = "Print details for debugging, like the tmux commands run.",
        long,
        short
    )]
    pub verbose: bool,
}

/// Returns the home directory of the current user.
fn home_dir() -> PathBuf {
    env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

/// Returns the XDG base directory in `$var`, or `$HOME/<fallback>` when it is not set to an
/// absolute path.
fn xdg_dir(var: &str, fallback: &str) -> PathBuf {
    env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home_dir().join(fallback))
}

/// Returns the default layout directory: `$XDG_CONFIG_HOME/tmux/layouts`.
pub fn default_layout_dir() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config").join("tmux/layouts")
}

/// Top-level subcommand dispatched by [`Program`].
#[derive(Debug, Subcommand)]
pub enum ProgramCommand {
    /// Apply a layout.
    #[command(
        name = "switch",
        after_help = SWITCH_EXAMPLES,
        about = "Apply a layout.",
        long_about = "Apply the layout <NAME>.yml (or <NAME>.yaml) from the layout directory.\n\nInside tmux, the windows of the layout are added to the current session. Outside tmux, a new session named session.name is created and attached; if it already exists, it is attached as is.\n\n${VAR} and $VAR references in the layout file are replaced with environment variables before it is read. A cwd may be set on the session, a window or a pane; the most specific one wins, and a leading ~ is your home directory. If IN_NIX_SHELL is set, every pane command runs in `nix develop`.",
        next_display_order = 1
    )]
    Switch(SwitchCommandArgs),

    /// Create a starter layout.
    #[command(
        name = "new",
        after_help = NEW_EXAMPLES,
        about = "Create a starter layout.",
        long_about = "Write a commented starter layout to <NAME>.yml in the layout directory, with the session named <NAME> and opening in the current directory. An existing layout is never replaced unless --force is given.",
        next_display_order = 2
    )]
    New(NewCommandArgs),

    /// Open a layout in your editor, then check it.
    #[command(
        name = "edit",
        after_help = EDIT_EXAMPLES,
        about = "Open a layout in your editor, then check it.",
        long_about = "Open the layout <NAME>.yml (or <NAME>.yaml) in $VISUAL or $EDITOR (vi if neither is set), and check that it is valid once the editor exits, the same way `switch` reads it.",
        next_display_order = 3
    )]
    Edit(EditCommandArgs),

    /// List the available layouts.
    #[command(
        name = "list",
        after_help = LIST_EXAMPLES,
        about = "List the available layouts.",
        long_about = "Print the name of every *.yml and *.yaml file in the layout directory, one per line.",
        next_display_order = 4
    )]
    List(ListCommandArgs),
}

impl ProgramCommand {
    /// Returns the shared global flags of the subcommand.
    pub fn parent(&self) -> &ProgramArgs {
        match self {
            ProgramCommand::Switch(args) => &args.parent,
            ProgramCommand::New(args) => &args.parent,
            ProgramCommand::Edit(args) => &args.parent,
            ProgramCommand::List(args) => &args.parent,
        }
    }
}

/// SwitchCommandArgs defines the arguments for the SwitchCommand.
#[derive(Debug, Args)]
pub struct SwitchCommandArgs {
    /// Shared global flags.
    #[command(flatten)]
    pub parent: ProgramArgs,

    /// Name of the layout to apply.
    #[arg(help = "Layout name (the file name without extension).")]
    pub name: String,
}

/// NewCommandArgs defines the arguments for the NewCommand.
#[derive(Debug, Args)]
pub struct NewCommandArgs {
    /// Shared global flags.
    #[command(flatten)]
    pub parent: ProgramArgs,

    /// Name of the layout to create.
    #[arg(help = "Layout name (the file name without extension).")]
    pub name: String,

    /// Replace an existing layout.
    #[arg(help = "Replace the layout if it exists.", long, short)]
    pub force: bool,
}

/// EditCommandArgs defines the arguments for the EditCommand.
#[derive(Debug, Args)]
pub struct EditCommandArgs {
    /// Shared global flags.
    #[command(flatten)]
    pub parent: ProgramArgs,

    /// Name of the layout to edit.
    #[arg(help = "Layout name (the file name without extension).")]
    pub name: String,
}

/// ListCommandArgs defines the arguments for the ListCommand.
#[derive(Debug, Args)]
pub struct ListCommandArgs {
    /// Shared global flags.
    #[command(flatten)]
    pub parent: ProgramArgs,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn program_is_valid() {
        Program::command().debug_assert();
    }

    #[test]
    fn switch_parses_the_name_and_flags() {
        let program = Program::parse_from(["tmux-layout", "switch", "-v", "-d", "/layouts", "dev"]);
        let ProgramCommand::Switch(args) = program.command else {
            panic!("expected switch");
        };
        assert_eq!(args.name, "dev");
        assert_eq!(args.parent.layout_dir, PathBuf::from("/layouts"));
        assert!(args.parent.verbose);
    }

    #[test]
    fn switch_requires_a_name() {
        assert!(Program::try_parse_from(["tmux-layout", "switch"]).is_err());
    }

    #[test]
    fn new_parses_force() {
        let program = Program::parse_from(["tmux-layout", "new", "dev", "--force"]);
        let ProgramCommand::New(args) = program.command else {
            panic!("expected new");
        };
        assert_eq!(args.name, "dev");
        assert!(args.force);
    }

    #[test]
    fn list_rejects_arguments() {
        assert!(Program::try_parse_from(["tmux-layout", "list", "garbage"]).is_err());
    }
}
