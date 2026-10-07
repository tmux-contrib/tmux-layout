# tmux-layout

> Stop hand-rolling tmux sessions. Declare your windows and panes in a YAML
> file once, then jump back into the same workspace any time with a single
> command — Nix dev shell aware, env-substitution included.

[![CI](https://github.com/tmux-contrib/tmux-layout/actions/workflows/ci.yml/badge.svg)](https://github.com/tmux-contrib/tmux-layout/actions/workflows/ci.yml) [![Release](https://img.shields.io/github/v/release/tmux-contrib/tmux-layout)](https://github.com/tmux-contrib/tmux-layout/releases) [![Rust](https://img.shields.io/badge/built_with-Rust-dea584?logo=rust)](https://www.rust-lang.org) [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

```sh
tmux-layout switch dev
```

## Install

A single binary; the only runtime dependency is `tmux`.

**Nix:**

```sh
nix run github:tmux-contrib/tmux-layout -- switch dev
# or install:
nix profile install github:tmux-contrib/tmux-layout
```

**Download:** each [release](https://github.com/tmux-contrib/tmux-layout/releases/latest)
has a binary per platform: `tmux-layout-aarch64-apple-darwin`,
`tmux-layout-x86_64-apple-darwin`, `tmux-layout-x86_64-unknown-linux-musl`
and `tmux-layout-aarch64-unknown-linux-musl`. The Linux binaries are static
and run on any distribution.

```sh
curl -fsSL --create-dirs -o ~/.local/bin/tmux-layout \
  https://github.com/tmux-contrib/tmux-layout/releases/latest/download/tmux-layout-aarch64-apple-darwin
chmod +x ~/.local/bin/tmux-layout
```

**Cargo** (from source):

```sh
cargo install --git https://github.com/tmux-contrib/tmux-layout
```

## Usage

Get started in a project directory:

```sh
tmux-layout new dev      # create ~/.config/tmux/layouts/dev.yml, opening here
tmux-layout edit dev     # declare your windows and panes
tmux-layout switch dev   # open them
```

Or start from the session you already have, from inside tmux:

```sh
tmux-layout save dev     # write the current session to ~/.config/tmux/layouts/dev.yml
```

```sh
tmux-layout switch <name>        # apply a layout
tmux-layout new <name> [--force] # create a commented starter layout
tmux-layout edit <name>          # open a layout in $VISUAL or $EDITOR, then check it
tmux-layout save <name> [-t <session>] [--force]
                                 # save a tmux session as a layout
tmux-layout list                 # list available layouts
tmux-layout --help
tmux-layout switch --help
```

- `new` writes a starter layout whose session is named `<name>` and opens in
  the current directory. It never replaces an existing layout unless `--force`
  is given.
- `edit` checks the layout the same way `switch` reads it once your editor
  exits, so mistakes show up right away instead of the next time you switch.
  If the layout is invalid, it offers to re-open the editor. Editors with
  arguments work, e.g. `EDITOR="code --wait"`.
- `list` shows a table of the layouts in a terminal: the session each one
  opens, how many windows and panes it has, its `cwd`, and why it is invalid
  if it is. `●` marks layouts whose session is running. Piped, it prints one
  name per line, e.g. to pick one with
  `tmux-layout switch "$(tmux-layout list | fzf --tmux)"`.
- `save` writes the session you are in, or the one given with `--target`
  (`-t`), as a layout `switch` opens again. Like `new`, it never replaces an
  existing layout unless `--force` is given. It saves:
  - the session name, and as `session.cwd` the directory most panes share;
  - window names (unless tmux names the window automatically), the exact
    `select-layout` string of windows with several panes, and a `cwd` when
    most of a window's panes share one that differs from the session's;
  - pane titles (unless it is the default, the host name), a pane `cwd` when
    it differs from its window's, and the command the pane was started with,
    or else the program it runs unless it is a shell. Only the name of a
    running program is known, not its arguments: `nvim`, not
    `nvim src/main.rs`.

  Paths in your home directory are written as `~/...`. Run
  `tmux-layout edit <name>` afterwards to adjust what was saved.

Layouts are read from `$XDG_CONFIG_HOME/tmux/layouts` (default
`~/.config/tmux/layouts`). Use `--layout-dir` (`-d`) or `TMUX_LAYOUT_DIR` to
read them from elsewhere, and `--verbose` (`-v`) to print the tmux commands
that are run.

## Layout file

`tmux-layout new` writes a commented starter layout. A complete one looks
like this, in `~/.config/tmux/layouts/dev.yml`:

```yaml
session:
  name: my-session-name
  cwd: ~/code # optional; default cwd for every pane
windows:
  - name: my-window-name
    layout: tiled # optional; passed to `tmux select-layout`
    cwd: ~/code/myapp # optional; overrides session.cwd for this window
    panes:
      - name: my-tig-pane # optional; sets pane title
        command: "tig" # optional; empty leaves pane in default shell
      - name: my-claude-pane
        command: "claude"
      - name: my-nvim-pane
        command: "nvim"
        cwd: ~/code/myapp/src # optional; overrides window.cwd for this pane
```

A layout may declare multiple `windows`, each with one or more `panes`.
The first pane is the window's initial pane; subsequent panes are
created via `tmux split-window`.

## Behavior

- **Inside tmux**: the layout's windows are appended to the current
  session.
- **Outside tmux**: a new session named `session.name` is created and
  attached. If the session already exists, it is attached as-is (no
  modification) — re-running is safe.
- **Working directory**: `cwd:` may be set at session, window, or pane
  level. Precedence is **pane > window > session**, so a window-level
  `cwd` applies to all its panes unless a pane overrides it. A leading
  `~` expands to `$HOME`; `$VAR` and `${VAR}` are substituted (see
  below); anything else is passed to `tmux -c` as-is (absolute or
  relative to wherever tmux is invoked).
- **Nix dev shells**: if `IN_NIX_SHELL` is set, every pane command is
  run as `nix develop -c "$SHELL" -c "<cmd>"` so tools defined in the
  dev shell remain available and shell features (pipes, `&&`, aliases)
  work inside the pane command.
- **Env substitution**: `$VAR` and `${VAR}` references in the YAML are
  replaced with environment variables before parsing, like `envsubst`
  (unset variables become empty), e.g.:

  ```yaml
  session:
    name: "${USER}-dev"
  windows:
    - name: editor
      panes:
        - command: "${EDITOR:-vim}"
  ```

  Substitution happens at parse time, not at command run time, so any
  `$VAR` in a `command:` is expanded when the layout is read (not by the
  shell at runtime). Other forms, like `${EDITOR:-vim}`, are left for the
  pane's shell to expand.

## License

[MIT](LICENSE).
