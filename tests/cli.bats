#!/usr/bin/env bats
# Smoke tests for the top-level CLI dispatch.

load helpers

@test "--version prints version.txt" {
	run "$TMUX_LAYOUT_BIN" --version
	[ "$status" -eq 0 ]
	[ "$output" = "$(cat "$PROJECT_ROOT/version.txt")" ]
}

@test "-V is an alias for --version" {
	run "$TMUX_LAYOUT_BIN" -V
	[ "$status" -eq 0 ]
	[ "$output" = "$(cat "$PROJECT_ROOT/version.txt")" ]
}

@test "--help describes the subcommands" {
	run "$TMUX_LAYOUT_BIN" --help
	[ "$status" -eq 0 ]
	[[ "$output" == *"switch <name>"* ]]
	[[ "$output" == *"list"* ]]
}

@test "no arguments prints help and exits zero" {
	run "$TMUX_LAYOUT_BIN"
	[ "$status" -eq 0 ]
	[[ "$output" == *"USAGE:"* ]]
}

@test "unknown subcommand exits nonzero with hint" {
	run "$TMUX_LAYOUT_BIN" bogus
	[ "$status" -ne 0 ]
	[[ "$output" == *"unknown command 'bogus'"* ]]
}

@test "switch --help works without tmux/yq installed" {
	# We can't actually uninstall tools here, but the contract is that
	# `switch --help` short-circuits before the dependency check.
	run "$TMUX_LAYOUT_BIN" switch --help
	[ "$status" -eq 0 ]
	[[ "$output" == *"tmux-layout switch"* ]]
}

@test "list --help works without tmux/yq installed" {
	run "$TMUX_LAYOUT_BIN" list --help
	[ "$status" -eq 0 ]
	[[ "$output" == *"tmux-layout list"* ]]
}
