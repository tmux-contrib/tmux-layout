#!/usr/bin/env bash
# Shared helpers for bats tests.

PROJECT_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/.." && pwd)"
export PROJECT_ROOT

# Path-resolve the tmux-layout binary for tests.
TMUX_LAYOUT_BIN="$PROJECT_ROOT/tmux-layout"
export TMUX_LAYOUT_BIN

# Set up an isolated XDG_CONFIG_HOME so layout files don't collide with
# the user's actual ~/.config.
setup_xdg() {
	export XDG_CONFIG_HOME="$BATS_TEST_TMPDIR/config"
	mkdir -p "$XDG_CONFIG_HOME/tmux/layouts"
	# Running the suite from inside `nix develop` exports IN_NIX_SHELL,
	# which would otherwise make the script prefix every pane command
	# with `nix develop -c`. Tests should opt into that explicitly.
	unset IN_NIX_SHELL
}

# Set up an isolated tmux server (own socket dir, own socket name).
# Subsequent `tmux` invocations from anywhere in this test will hit the
# isolated server because we override TMUX_TMPDIR.
setup_tmux_sandbox() {
	export TMUX_TMPDIR="$BATS_TEST_TMPDIR/tmux"
	mkdir -p "$TMUX_TMPDIR"
	# Make absolutely sure no inherited $TMUX leaks into "outside tmux" tests.
	unset TMUX
}

# Tear down the isolated tmux server.
teardown_tmux_sandbox() {
	if [ -n "${TMUX_TMPDIR:-}" ] && [ -d "$TMUX_TMPDIR" ]; then
		tmux kill-server 2>/dev/null || true
	fi
}

# Write a layout file at $XDG_CONFIG_HOME/tmux/layouts/<name>.yml
write_layout() {
	local name=$1
	local body=$2
	printf '%s\n' "$body" >"$XDG_CONFIG_HOME/tmux/layouts/$name.yml"
}
