#!/usr/bin/env bats
# Integration tests for `switch` against a real, isolated tmux server.

load helpers

setup() {
	setup_xdg
	setup_tmux_sandbox
}

teardown() {
	teardown_tmux_sandbox
}

@test "switch with no argument errors" {
	run "$TMUX_LAYOUT_BIN" switch
	[ "$status" -ne 0 ]
	[[ "$output" == *"missing layout name"* ]]
}

@test "switch rejects an unknown flag" {
	run "$TMUX_LAYOUT_BIN" switch --bogus
	[ "$status" -ne 0 ]
	[[ "$output" == *"unknown flag"* ]]
}

@test "switch errors when the layout is missing" {
	run "$TMUX_LAYOUT_BIN" switch nope
	[ "$status" -ne 0 ]
	[[ "$output" == *"layout 'nope' not found"* ]]
}

@test "switch errors when session.name is missing" {
	write_layout bad "windows: [{name: w, panes: [{command: tig}]}]"
	run "$TMUX_LAYOUT_BIN" switch bad
	[ "$status" -ne 0 ]
	[[ "$output" == *"session.name is required"* ]]
}

@test "switch errors when no windows are declared" {
	write_layout bad "session: {name: x}"$'\n'"windows: []"
	run "$TMUX_LAYOUT_BIN" switch bad
	[ "$status" -ne 0 ]
	[[ "$output" == *"at least one window"* ]]
}

@test "switch creates a new session with the configured windows" {
	write_layout demo "$(cat <<'YAML'
session:
  name: t-demo
windows:
  - name: editor
    panes:
      - command: "sleep 100"
      - command: "sleep 100"
  - name: notes
    panes:
      - command: "sleep 100"
YAML
)"
	# `exec tmux attach-session` in the script will fail in this non-TTY
	# environment, but the session is created before the attach. Ignore the
	# exit status and verify out-of-band.
	"$TMUX_LAYOUT_BIN" switch demo || true

	run tmux has-session -t '=t-demo'
	[ "$status" -eq 0 ]

	run tmux list-windows -t '=t-demo' -F '#{window_name}'
	[ "$status" -eq 0 ]
	# Order: editor first (bootstrapped via new-session), then notes
	[ "${lines[0]}" = "editor" ]
	[ "${lines[1]}" = "notes" ]
	[ "${#lines[@]}" -eq 2 ]

	run tmux list-panes -t '=t-demo:editor' -F '#{pane_id}'
	[ "$status" -eq 0 ]
	[ "${#lines[@]}" -eq 2 ]

	run tmux list-panes -t '=t-demo:editor' -F '#{pane_id}'
	[ "$status" -eq 0 ]
	[ "${#lines[@]}" -eq 2 ]
}

@test "switch is idempotent when the session already exists" {
	write_layout demo "$(cat <<'YAML'
session:
  name: t-idem
windows:
  - name: only
    panes:
      - command: "sleep 100"
YAML
)"
	"$TMUX_LAYOUT_BIN" switch demo || true
	# Second invocation should attach to the existing session, not duplicate windows.
	run "$TMUX_LAYOUT_BIN" switch demo
	# The attach-session will fail in headless mode, but the script should
	# have printed the "already exists" hint to stderr before exec'ing.
	[[ "$output" == *"already exists"* ]]

	run tmux list-windows -t '=t-idem' -F '#{window_name}'
	[ "$status" -eq 0 ]
	[ "${#lines[@]}" -eq 1 ]
	[ "${lines[0]}" = "only" ]
}

@test "switch substitutes \${VAR} in YAML values via envsubst" {
	write_layout envtest "$(cat <<'YAML'
session:
  name: "t-${TEST_SESSION_SUFFIX}"
windows:
  - name: w
    panes:
      - command: "sleep 100"
YAML
)"
	TEST_SESSION_SUFFIX=substituted \
		"$TMUX_LAYOUT_BIN" switch envtest || true

	run tmux has-session -t '=t-substituted'
	[ "$status" -eq 0 ]
}

@test "switch appends windows to the current session when invoked from inside tmux" {
	# Create an outer host session.
	tmux new-session -d -s host -n original
	# Launch our script inside a new pane of that session, which inherits $TMUX.
	# We do this by spawning a window whose initial command IS our script.
	write_layout append "$(cat <<'YAML'
session:
  name: ignored-when-inside-tmux
windows:
  - name: appended-1
    panes:
      - command: "sleep 100"
  - name: appended-2
    panes:
      - command: "sleep 100"
YAML
)"
	tmux new-window -t host -n runner \
		"XDG_CONFIG_HOME='$XDG_CONFIG_HOME' '$TMUX_LAYOUT_BIN' switch append; sleep 5"
	# Poll briefly for the appended windows to appear.
	local tries=0
	while ((tries < 30)); do
		if tmux list-windows -t host -F '#{window_name}' | grep -qx appended-2; then
			break
		fi
		sleep 0.1
		((++tries))
	done

	run tmux list-windows -t host -F '#{window_name}'
	[ "$status" -eq 0 ]
	# Should include the original window plus the two appended ones.
	[[ "$output" == *"original"* ]]
	[[ "$output" == *"appended-1"* ]]
	[[ "$output" == *"appended-2"* ]]
	# Crucially: no new top-level session was created.
	run tmux has-session -t '=ignored-when-inside-tmux'
	[ "$status" -ne 0 ]
}
