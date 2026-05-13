# tmux-layout

A small CLI that applies predefined tmux layouts from a YAML file in
`~/.config/tmux/layouts/<name>.yml`.

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
windows:
  - name: my-window-name
    layout: tiled               # optional; passed to `tmux select-layout`
    panes:
      - name: my-tig-pane       # optional; sets pane title
        command: "tig"          # optional; empty leaves pane in default shell
      - name: my-claude-pane
        command: "claude"
      - name: my-nvim-pane
        command: "nvim"
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
- **Nix dev shells**: if `IN_NIX_SHELL` is set, every pane command is
  prefixed with `nix develop -c` so tools defined in the dev shell
  remain available in each pane.
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
