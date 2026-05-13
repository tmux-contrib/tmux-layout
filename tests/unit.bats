#!/usr/bin/env bats
# Unit tests for pure helper functions (no tmux required).

load helpers

setup() {
	setup_xdg
	# Source the helpers module directly. The top-level `set -euo pipefail`
	# and `readonly _TMUX_LAYOUT_DIR=...` run once here.
	# shellcheck disable=SC1090
	source "$PROJECT_ROOT/scripts/tmux_layout_switch.sh"
}

@test "_resolve_layout finds <name>.yml" {
	write_layout demo "session: {name: x}"
	run _resolve_layout demo
	[ "$status" -eq 0 ]
	[ "$output" = "$XDG_CONFIG_HOME/tmux/layouts/demo.yml" ]
}

@test "_resolve_layout falls back to <name>.yaml" {
	mv "$XDG_CONFIG_HOME/tmux/layouts" "$XDG_CONFIG_HOME/tmux/_old" || true
	mkdir -p "$XDG_CONFIG_HOME/tmux/layouts"
	printf 'session: {name: x}\n' >"$XDG_CONFIG_HOME/tmux/layouts/demo.yaml"
	run _resolve_layout demo
	[ "$status" -eq 0 ]
	[ "$output" = "$XDG_CONFIG_HOME/tmux/layouts/demo.yaml" ]
}

@test "_resolve_layout errors when layout is missing" {
	run _resolve_layout nope
	[ "$status" -ne 0 ]
	[[ "$output" == *"layout 'nope' not found"* ]]
}

@test "_build_pane_cmd returns empty for empty input" {
	run _build_pane_cmd ""
	[ "$status" -eq 0 ]
	[ -z "$output" ]
}

@test "_build_pane_cmd passes the command through when not in nix-shell" {
	unset IN_NIX_SHELL
	run _build_pane_cmd "tig"
	[ "$status" -eq 0 ]
	[ "$output" = "tig" ]
}

@test "_build_pane_cmd wraps with 'nix develop -c \$SHELL -c' inside a nix-shell" {
	IN_NIX_SHELL=pure SHELL=/bin/sh run _build_pane_cmd "tig"
	[ "$status" -eq 0 ]
	[ "$output" = "nix develop -c /bin/sh -c tig" ]
}

@test "_build_pane_cmd shell-escapes commands with metacharacters" {
	IN_NIX_SHELL=pure SHELL=/bin/sh run _build_pane_cmd "tig --all | less"
	[ "$status" -eq 0 ]
	# The pipe must be inside the escaped argument, not a token in the outer
	# string — parse the output back through `eval` and confirm we get exactly
	# six args with the command intact.
	eval "set -- $output"
	[ "$1" = "nix" ]
	[ "$2" = "develop" ]
	[ "$3" = "-c" ]
	[ "$4" = "/bin/sh" ]
	[ "$5" = "-c" ]
	[ "$6" = "tig --all | less" ]
	[ "$#" -eq 6 ]
}

@test "_expand_cwd returns empty for empty input" {
	run _expand_cwd ""
	[ "$status" -eq 0 ]
	[ -z "$output" ]
}

@test "_expand_cwd expands a bare ~ to \$HOME" {
	HOME=/h run _expand_cwd "~"
	[ "$status" -eq 0 ]
	[ "$output" = "/h" ]
}

@test "_expand_cwd expands ~/foo to \$HOME/foo" {
	HOME=/h run _expand_cwd "~/foo/bar"
	[ "$status" -eq 0 ]
	[ "$output" = "/h/foo/bar" ]
}

@test "_expand_cwd passes absolute paths through" {
	run _expand_cwd "/etc/foo"
	[ "$status" -eq 0 ]
	[ "$output" = "/etc/foo" ]
}

@test "_expand_cwd passes relative paths through" {
	run _expand_cwd "./services/api"
	[ "$status" -eq 0 ]
	[ "$output" = "./services/api" ]
}

@test "_expand_cwd leaves ~user unchanged (unsupported form)" {
	run _expand_cwd "~root/foo"
	[ "$status" -eq 0 ]
	[ "$output" = "~root/foo" ]
}

@test "_resolve_cwd: pane overrides window overrides session" {
	HOME=/h
	_tmux_layout_yaml=$'session:\n  cwd: ~/s\nwindows:\n  - cwd: ~/w\n    panes:\n      - {cwd: ~/p}\n      - {}\n'
	run _resolve_cwd 0 0
	[ "$status" -eq 0 ]
	[ "$output" = "/h/p" ]
}

@test "_resolve_cwd: falls back to window cwd when pane has none" {
	HOME=/h
	_tmux_layout_yaml=$'session:\n  cwd: ~/s\nwindows:\n  - cwd: ~/w\n    panes:\n      - {}\n'
	run _resolve_cwd 0 0
	[ "$status" -eq 0 ]
	[ "$output" = "/h/w" ]
}

@test "_resolve_cwd: falls back to session cwd when window and pane have none" {
	HOME=/h
	_tmux_layout_yaml=$'session:\n  cwd: ~/s\nwindows:\n  - panes: [{}]\n'
	run _resolve_cwd 0 0
	[ "$status" -eq 0 ]
	[ "$output" = "/h/s" ]
}

@test "_resolve_cwd: empty when nothing is set" {
	_tmux_layout_yaml=$'session: {name: x}\nwindows:\n  - panes: [{}]\n'
	run _resolve_cwd 0 0
	[ "$status" -eq 0 ]
	[ -z "$output" ]
}

@test "_yaml_get returns the field value" {
	_tmux_layout_yaml=$'session:\n  name: my-session\n'
	run _yaml_get '.session.name'
	[ "$status" -eq 0 ]
	[ "$output" = "my-session" ]
}

@test "_yaml_get returns empty string for missing field" {
	_tmux_layout_yaml=$'session:\n  name: my-session\n'
	run _yaml_get '.session.missing'
	[ "$status" -eq 0 ]
	[ -z "$output" ]
}

@test "_yaml_get returns empty string for explicit null" {
	_tmux_layout_yaml=$'session:\n  name: null\n'
	run _yaml_get '.session.name'
	[ "$status" -eq 0 ]
	[ -z "$output" ]
}

@test "_yaml_get_int returns array length" {
	_tmux_layout_yaml=$'windows:\n  - {name: a}\n  - {name: b}\n  - {name: c}\n'
	run _yaml_get_int '.windows | length'
	[ "$status" -eq 0 ]
	[ "$output" = "3" ]
}
