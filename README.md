# tmux-layout

> Stop hand-rolling tmux sessions. Declare your windows and panes in a YAML
> file once, then jump back into the same workspace any time with a single
> command — Nix dev shell aware, env-substitution included.

[![CI](https://github.com/tmux-contrib/tmux-layout/actions/workflows/ci.yml/badge.svg)](https://github.com/tmux-contrib/tmux-layout/actions/workflows/ci.yml) [![Release](https://img.shields.io/github/v/release/tmux-contrib/tmux-layout)](https://github.com/tmux-contrib/tmux-layout/releases) [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

```sh
tmux-layout switch dev
```

## Install

### Zsh plugin

```zsh
# zinit
zinit light tmux-contrib/tmux-layout

# antidote (in ~/.zsh_plugins.txt)
tmux-contrib/tmux-layout

# oh-my-zsh
git clone https://github.com/tmux-contrib/tmux-layout \
  "${ZSH_CUSTOM:-$HOME/.oh-my-zsh/custom}/plugins/tmux-layout"
# then add `tmux-layout` to plugins=(...) in ~/.zshrc
```

The plugin file simply prepends the repo directory to `$PATH` so the
`tmux-layout` script becomes available as a command.

### Nix

```sh
nix run github:tmux-contrib/tmux-layout -- switch dev
# or install:
nix profile install github:tmux-contrib/tmux-layout
```

### Manual

```sh
git clone https://github.com/tmux-contrib/tmux-layout
ln -s "$PWD/tmux-layout/tmux-layout" /usr/local/bin/tmux-layout
```

### Dependencies

- Bash 4.4+
- `tmux`
- [`yq`](https://github.com/mikefarah/yq) (Go, mikefarah/yq) —
  `brew install yq` or `nix profile install nixpkgs#yq-go`
- `envsubst` (from gettext) — preinstalled on most Linux/Nix systems;
  macOS: `brew install gettext`

## Usage

```sh
tmux-layout switch <name>    # apply a layout
tmux-layout list             # list available layouts
tmux-layout --help
tmux-layout switch --help
```

## Layout file

`~/.config/tmux/layouts/dev.yml`:

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
  `~` expands to `$HOME`; `${VAR}` forms are expanded via `envsubst`
  (see below); anything else is passed to `tmux -c` as-is (absolute or
  relative to wherever tmux is invoked).
- **Nix dev shells**: if `IN_NIX_SHELL` is set, every pane command is
  run as `nix develop -c "$SHELL" -c "<cmd>"` so tools defined in the
  dev shell remain available and shell features (pipes, `&&`, aliases)
  work inside the pane command.
- **Env substitution**: `${VAR}` references in the YAML are expanded
  via `envsubst` before parsing, e.g.:

  ```yaml
  session:
    name: "${USER}-dev"
  windows:
    - name: editor
      panes:
        - command: "${EDITOR:-vim}"
  ```

  Substitution happens at parse time, not at command run time, so any
  `$VAR` in a `command:` is expanded by `envsubst` (not by the shell at
  runtime).

## License

[MIT](LICENSE).
