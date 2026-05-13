#!/usr/bin/env bats
# Tests for the `list` subcommand.

load helpers

setup() {
	setup_xdg
}

@test "list errors when the layout directory does not exist" {
	rm -rf "$XDG_CONFIG_HOME/tmux/layouts"
	run "$TMUX_LAYOUT_BIN" list
	[ "$status" -ne 0 ]
	[[ "$output" == *"does not exist"* ]]
}

@test "list errors when the layout directory is empty" {
	run "$TMUX_LAYOUT_BIN" list
	[ "$status" -ne 0 ]
	[[ "$output" == *"no layouts"* ]]
}

@test "list prints layout names without extension" {
	write_layout dev "session: {name: x}"
	write_layout work "session: {name: y}"
	run "$TMUX_LAYOUT_BIN" list
	[ "$status" -eq 0 ]
	# Order is alphabetic by glob; assert both names present.
	[[ "$output" == *"dev"* ]]
	[[ "$output" == *"work"* ]]
	# No file extensions in the output
	[[ "$output" != *".yml"* ]]
	[[ "$output" != *".yaml"* ]]
}

@test "list rejects unexpected positional args" {
	write_layout dev "session: {name: x}"
	run "$TMUX_LAYOUT_BIN" list garbage
	[ "$status" -ne 0 ]
	[[ "$output" == *"unexpected argument"* ]]
}
