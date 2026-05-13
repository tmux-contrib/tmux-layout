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

@test "_build_pane_cmd prefixes with 'nix develop -c' inside a nix-shell" {
	IN_NIX_SHELL=pure run _build_pane_cmd "tig"
	[ "$status" -eq 0 ]
	[ "$output" = "nix develop -c tig" ]
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
